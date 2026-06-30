//! Nexora Storage demo — demonstrates SQLite persistence.
//!
//! Shows that data survives a simulated restart:
//! 1. Opens a SQLite database
//! 2. Creates users, publishes events, publishes packages
//! 3. "Restarts" (drops all in-memory state)
//! 4. Reopens the database and loads everything back
//! 5. Verifies all data survived

use nexora_auth::UserStore;
use nexora_core::NexoraCore;
use nexora_core::events::EventPayload;
use nexora_marketplace::package::{PackageBilling, PackageManifest, PackageType, ResourceLimits, Visibility};
use nexora_marketplace::store::PackageStore;
use nexora_marketplace::version::{Version, VersionRange};
use nexora_storage::{Database, SqliteEventStore, SqlitePackageStore, SqliteUserStore};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    println!("=== Nexora Storage Persistence Demo ===\n");

    let db_path = "/tmp/nexora-storage-demo.db";

    // Clean up any previous run.
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path));
    let _ = std::fs::remove_file(format!("{}-shm", db_path));

    // ---- Phase 1: Write data ----
    println!("[Phase 1] Opening database and writing data...");
    let db = Database::open(db_path)?;
    let core = Arc::new(NexoraCore::new());
    let event_bus = core.events_inner();
    let perm_engine = core.permissions_inner();

    let sql_users = SqliteUserStore::new(db.clone())
        .with_permission_engine(perm_engine.clone())
        .with_event_bus(event_bus.clone());
    let sql_events = SqliteEventStore::new(db.clone(), event_bus.clone());
    let sql_packages = SqlitePackageStore::new(db.clone()).with_event_bus(event_bus.clone());

    let mem_users = UserStore::new()
        .with_permission_engine(perm_engine.clone())
        .with_event_bus(event_bus.clone());
    let mem_packages = PackageStore::new().with_event_bus(event_bus.clone());

    // Create users.
    let alice = sql_users.create(&mem_users, "alice", "hunter2", Some("alice@nexora.io".into()), vec!["admin".into()])?;
    let bob = sql_users.create(&mem_users, "bob", "secret", None, vec!["viewer".into()])?;
    println!("  Created users: {} ({}), {} ({})", alice.username, alice.id, bob.username, bob.id);

    // Publish events.
    let eid1 = sql_events.publish("user.created", EventPayload::Text(alice.id.clone()));
    let eid2 = sql_events.publish("user.created", EventPayload::Text(bob.id.clone()));
    let eid3 = sql_events.publish("user.logged_in", EventPayload::Text(alice.id.clone()));
    println!("  Published events: #{}, #{}, #{}", eid1, eid2, eid3);

    // Publish a package.
    let manifest = PackageManifest {
        id: "com.nexora.demo.auth".into(),
        name: "Demo Auth Module".into(),
        version: Version::new(1, 0, 0),
        package_type: PackageType::Module,
        owner_public_key: "00".repeat(32),
        owner_name: "Nexora".into(),
        capabilities: vec!["nxp.command.execute".into()],
        resource_limits: ResourceLimits::default(),
        dependencies: vec![],
        nxp_capabilities: vec!["quic".into()],
        core_compatibility: VersionRange::Caret(Version::new(0, 1, 0)),
        billing: PackageBilling::Free,
        visibility: Visibility::Public,
        signature: "ff".repeat(64),
        description: "Demo auth module".into(),
        readme: "# Demo".into(),
        tags: vec!["auth".into(), "demo".into()],
    };
    let pkg = sql_packages.publish(&mem_packages, manifest)?;
    println!("  Published package: {}@{}", pkg.manifest.id, pkg.manifest.version);

    // Install it.
    sql_packages.mark_installed(&mem_packages, "com.nexora.demo.auth", &Version::new(1, 0, 0))?;
    println!("  Installed package");

    println!("  SQLite counts: {} users, {} events, {} package versions",
        sql_users.count()?, sql_events.count()?, sql_packages.count()?);

    // ---- Phase 2: Simulate restart ----
    println!("\n[Phase 2] Simulating restart (dropping all in-memory state)...");
    drop(sql_users);
    drop(sql_events);
    drop(sql_packages);
    drop(mem_users);
    drop(mem_packages);
    drop(core);
    drop(db);
    println!("  All in-memory state dropped.");

    // ---- Phase 3: Reload from SQLite ----
    println!("\n[Phase 3] Reopening database and loading data...");
    let db2 = Database::open(db_path)?;
    let core2 = Arc::new(NexoraCore::new());
    let event_bus2 = core2.events_inner();
    let perm_engine2 = core2.permissions_inner();

    let sql_users2 = SqliteUserStore::new(db2.clone())
        .with_permission_engine(perm_engine2.clone())
        .with_event_bus(event_bus2.clone());
    let sql_events2 = SqliteEventStore::new(db2.clone(), event_bus2.clone());
    let sql_packages2 = SqlitePackageStore::new(db2.clone()).with_event_bus(event_bus2.clone());

    let mem_users2 = UserStore::new()
        .with_permission_engine(perm_engine2)
        .with_event_bus(event_bus2.clone());
    let mem_packages2 = PackageStore::new().with_event_bus(event_bus2);

    // Load from SQLite.
    let users_loaded = sql_users2.load_into(&mem_users2)?;
    let events_loaded = sql_events2.load_into_memory()?;
    let packages_loaded = sql_packages2.load_into(&mem_packages2)?;

    println!("  Loaded {} users, {} events, {} package versions from SQLite",
        users_loaded, events_loaded, packages_loaded);

    // ---- Phase 4: Verify ----
    println!("\n[Phase 4] Verifying data survived...");

    // Verify users.
    let alice2 = mem_users2.get_by_username("alice").expect("alice should exist");
    assert_eq!(alice2.username, "alice");
    assert_eq!(alice2.email, Some("alice@nexora.io".into()));
    assert_eq!(alice2.roles, vec!["admin".to_string()]);
    println!("  ✓ User alice restored (id={}, roles={:?})", alice2.id, alice2.roles);

    let bob2 = mem_users2.get_by_username("bob").expect("bob should exist");
    assert_eq!(bob2.username, "bob");
    println!("  ✓ User bob restored (id={})", bob2.id);

    // Verify password still works.
    assert!(mem_users2.verify("alice", "hunter2").is_ok());
    assert!(mem_users2.verify("alice", "WRONG").is_err());
    println!("  ✓ Password verification works (Argon2 hash survived)");

    // Verify events.
    let events = sql_events2.replay(0, None);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].name, "user.created");
    assert_eq!(events[1].name, "user.created");
    assert_eq!(events[2].name, "user.logged_in");
    println!("  ✓ {} events replayed from SQLite", events.len());

    // Verify packages.
    let pkg2 = mem_packages2.get_latest("com.nexora.demo.auth").expect("package should exist");
    assert_eq!(pkg2.manifest.name, "Demo Auth Module");
    assert!(pkg2.installed);
    assert_eq!(pkg2.install_count, 1);
    println!("  ✓ Package {}@{} restored (installed={}, install_count={})",
        pkg2.manifest.id, pkg2.manifest.version, pkg2.installed, pkg2.install_count);

    // Verify principals were re-registered.
    let principal_count = core2.permissions.principal_count();
    assert_eq!(principal_count, 2);
    println!("  ✓ {} principals re-registered in Permission Engine", principal_count);

    println!("\n=== All data survived the restart! ===");
    println!("\nThis demonstrates that Nexora's storage layer provides full");
    println!("durability for users, events (source of truth), and packages.");
    println!("SQLite is used for Tier-1 (Edge); swap with PostgreSQL for Tier 2/3.");

    // Cleanup.
    drop(db2);
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path));
    let _ = std::fs::remove_file(format!("{}-shm", db_path));

    Ok(())
}
