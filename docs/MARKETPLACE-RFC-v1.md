# Nexora Marketplace — RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification
**Implements:** Nexora Engineering Specification, Part 5 (Ecosystem & Marketplace)

---

## 1. Abstract

The Nexora Marketplace is NOT a simple store — it is a **full Software Economy
Layer**. It allows developers to build, publish, distribute, monetize, and
manage packages: Modules, Plugins, AI Agents, Templates, Services,
Automations.

Every package runs inside Nexora Core and communicates through NXP.

## 2. Package Model

Every package MUST include:
- Package ID (unique, slug-style)
- Name
- Version (strict SemVer)
- Type (Module / Plugin / AI / Template / Service / Automation)
- Owner identity (Ed25519 public key)
- Digital signature (Ed25519 over canonical manifest)
- Permissions manifest
- Resource limits
- Dependencies graph
- Compatibility matrix
- Runtime requirements
- NXP capabilities required
- Billing model
- Integrity hash (SHA-256)

## 3. Package Types

| Type | Description |
|------|-------------|
| Module | Full system component (Auth, Billing, CRM, ERP, AI Orchestrator) |
| Plugin | Extends a module without modifying core logic (sandboxed) |
| AI Agent | Autonomous system executing tasks via NXP (deferred — Part 11) |
| Template | Prebuilt system (SaaS starter, dashboard, full-stack kit) |
| Service | Deployable runtime (DB, worker, API, microservice) |
| Automation | Workflow-based logic (CI/CD, billing automation, deployment) |

## 4. Security Model

No package may execute without:
1. **Digital signature validation** (Ed25519)
2. **Integrity hash verification** (SHA-256)
3. **Permission review** (declared capabilities vs. requested)
4. **Resource budget approval** (CPU, memory, command rate)
5. **Sandbox execution test** (simulated run before commit)

Unsigned packages are strictly forbidden.

## 5. Installation Pipeline (13 steps)

Per RFC §"INSTALLATION PIPELINE":

1. Fetch package metadata
2. Verify signature
3. Validate dependencies
4. Simulate execution
5. Security scan
6. Resource estimation
7. Compatibility check
8. Sandbox test run
9. Approval check
10. Deploy into Core
11. Register with Service Registry
12. Enable NXP communication
13. Activate monitoring

If any step fails → installation is rejected.

## 6. Versioning (SemVer)

Strict `MAJOR.MINOR.PATCH`:
- MAJOR: breaking changes (require migration)
- MINOR: backward-compatible features
- PATCH: backward-compatible fixes

Multiple versions can coexist. Backward compatibility enforced when possible.

## 7. Dependency System

Packages may depend on:
- Other packages (by ID + version range)
- Core modules
- NXP capabilities
- External APIs (restricted)

The dependency graph MUST be:
- **Acyclic** (no circular dependencies)
- Verified at install time
- Locked per environment

## 8. Monetization

| Model | Description |
|-------|-------------|
| One-time purchase | Single payment |
| Subscription | Recurring (monthly/yearly) |
| Usage-based | Per NXP command / per event |
| Enterprise licensing | Custom terms |
| Revenue sharing | Platform takes a percentage |
| Developer royalties | Original author gets a cut of resales |

## 9. Rating & Trust System

Each package has:
- Security score (0-100)
- Performance score (0-100)
- Stability score (0-100)
- Community rating (1-5 stars)
- Enterprise rating (1-5 stars)
- Adoption metrics (install count, active install count)

Low-trust packages (security < 50) are automatically sandbox-restricted.

## 10. Distribution

| Visibility | Description |
|------------|-------------|
| Public | Anyone can install |
| Private | Only the owner |
| Organization-only | Only members of a specific org |
| Enterprise-only | Only enterprise customers |
| Region-restricted | Only specific geographic regions |

## 11. Auto-Update

Packages may support:
- Auto-update (latest compatible version)
- Scheduled update (window-based)
- Manual update approval
- Rollback to any prior version

Updates are always validated before deployment (same 13-step pipeline).

## 12. Compliance

| Spec Section | Status |
|--------------|--------|
| Part 5 — Package model | ✅ |
| Part 5 — 6 package types | ✅ |
| Part 5 — Security model (5 layers) | ✅ |
| Part 5 — Installation pipeline (13 steps) | ✅ |
| Part 5 — SemVer | ✅ |
| Part 5 — Dependency system (acyclic) | ✅ |
| Part 5 — Monetization (6 models) | ✅ |
| Part 5 — Rating & trust | ✅ |
| Part 5 — Distribution (5 visibilities) | ✅ |
| Part 5 — Auto-update | ⏳ Pending v0.2 |
| Part 5 — AI integration | ⏳ Deferred (Part 11) |

## 13. Test Coverage

(see crate tests for current count)
