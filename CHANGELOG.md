# سجل التغييرات

جميع التغييرات الجديرة بالملاحظة في مشروع Nexora سيتم توثيقها في هذا الملف.

التنسيق مبني على [احتفظ بسجل التغييرات](https://keepachangelog.com/ar-1.0.0/)
وهذا المشروع يلتزم بـ [الإصدار الدلالي](https://semver.org/lang/ar/).

## [v1.6.4] — 2026-07-05

### أُصلِح (clippy errors)
- **nexora-auth/src/token.rs**: إزالة `to_string()` الجوهرية التي كانت تُخفي
  `impl Display` (clippy error: `should_implement_trait`). أُعيدت تسميتها إلى
  `encode()`. `Display` ينتج نفس الناتج، لذا `token.to_string()` ما زال يعمل.
- **nexora-auth/src/sso/saml.rs**: إزالة حلقة `while` ميتة كانت تنكسر فوراً
  (clippy error: `this loop never actually loops`). الكود الفعلي كان في حلقة
  ثانية بعدها مباشرة.

### حُسّن (clippy warnings)
- **nexora-marketplace**: إزالة 6 imports غير مستخدمة في `handler.rs`،
  `install.rs`، `signature.rs`، `store.rs`
- **nexora-graphql/src/schema.rs**: إزالة `EventSubscriber` غير المستخدم
- **nexora-loadtest/src/runner.rs**: إزالة `Arc` و `Duration` غير المستخدمين
  + تسمية `per_worker` بـ `_per_worker`
- **nexora-marketplace/src/install.rs**: إزالة `mut` غير الضروري من معامل `steps`

### أُضيف
- **scripts/smoke-test.sh**: سكربت اختبار شامل للـ 13 endpoint رئيسي
  (Health, Login, Ping, Events, Marketplace, Cluster, Workflows, Notifications,
  Billing, GraphQL, OpenAPI, Auth-fail, Wrong-password)

### التحقق
- ✅ `cargo check --workspace --locked` — نظيف
- ✅ 872 اختبار يناج (17+4+13+6+2+42+85+66+17+21+24+74+9+26+22+51+40+90+68+46+21+10+0+128)
- ✅ `cargo clippy` — 0 أخطاء (بعض التحذيرات غير الحرجة متبقية)
- ✅ بناء Frontend محلياً يُنتج `build/index.html` بنجاح
- ✅ بناء `gateway-demo` release (9.8MB)
- ✅ 13/13 smoke test يناج على البوابة الحيّة

## [v1.6.3] — 2026-07-05

### أُصلِح (حرج)
- **healthcheck**: ثلاثة أخطاء في الفحص السابق جعلت backend دائماً "unhealthy":
  1. المسار كان `/health` بينما الصحيح `/api/health`
     (`crates/nexora-gateway/src/server.rs:127`)
  2. `debian:bookworm-slim` لا يحتوي على `curl` — الشرط
     `command -v curl >/dev/null 2>&1 && curl ...` كان يفشل صامتاً
  3. `start_period=30s` لم يكن كافياً لتهيئة tracing + bootstrap admin/viewer +
     publish demo package. رُفع إلى `60s` مع 5 retries بفاصل 15s
- **Dockerfile.backend**: تثبيت `curl` صراحةً في runtime stage
- **docker-compose.yml**: مزامنة healthcheck مع Dockerfile

## [v1.6.2] — 2026-07-05

### أُصلِح (حرج)
- **frontend/svelte.config.js**: استبدال `@sveltejs/adapter-auto` بـ
  `@sveltejs/adapter-static` — adapter-auto لا يُنشئ مجلد `build/` إلا في
  منصات معروفة (Vercel/Netlify)، مما جعل بناء Docker للواجهة يفشل صامتاً
  بسبب سطر `npm run build || echo "WARN..."` الذي يخفي الأخطاء
- **Dockerfile.frontend**: إزالة `|| echo "WARN..."` (البناء يفشل بصوت عالٍ
  الآن عند وجود أخطاء) + إضافة تحقق `test -f build/index.html`

### الاختبار
- تم اختبار البناء محلياً بـ Node 24 + npm 11 — يُنتج `build/index.html` ✓
- SPA fallback يعمل عبر `try_files $uri $uri/ /index.html` في nginx

## [v1.6.1] — 2026-07-05

### أُصلِح
- **Dockerfile.backend**: تثبيت Rust 1.89-bookworm بدلاً من `latest` للاستقرار
  (async-graphql 7.x و time 0.3.53 تتطلب rustc ≥ 1.88، asynk-strim يتطلب 1.89)
- إضافة `cargo-chef` لتخزين الاعتماديات مؤقتاً (تسريع 10x عند إعادة البناء)
- إضافة `--locked` لضمان استخدام Cargo.lock كما هو (بناء متكرر)
- تشغيل البوابة بمستخدم غير root (تحسين أمني)
- إضافة `HEALTHCHECK` للبوابة عبر `/health`

### أُضيف
- **`.dockerignore`**: استبعاد target/، node_modules/، .git/ لتسريعBuildContext
- **`scripts/docker-build.sh`**: سكربت بناء وتشغيل موحّد (build/up/down/logs/clean/rebuild)
- **`.env.example`**: نموذج للمتغيرات البيئية للإنتاج

### حُسّن
- **Dockerfile.frontend**: طبقة caching لـ package.json + healthcheck
- **nginx.conf**: ضغط gzip، حدود رفع 50MB، Security headers، healthz endpoint
- **docker-compose.yml**:
  - healthchecks لجميع الخدمات مع start_period
  - حدود موارد (memory + cpus) لكل خدمة
  - ترتيب بدء صحيح (depends_on مع condition)
  - متغير `NEXORA_JWT_SECRET` للبوابة

## [v1.6.0] — 2026-07-03

### أُضيف
- **توثيق عربي كامل**: ترجمة README، جميع وثائق RFC (6 ملفات)،
  وشروحات مستوى الـ crate لكل الـ 18 crate إلى العربية
- توثيق البوابة يشمل الآن مسارات الإشعارات و GraphQL

## [v1.5.0] — 2026-07-03

### أُضيف
- **nexora-notifications**: خدمة إشعارات متعددة القنوات
  - محول البريد (SMTP عبر lettre) مع TLS + مصادقة
  - محول دفع الويب (RFC 8291 + VAPID JWT) لإشعارات المتصفح
  - قناة داخل التطبيق مع تخزين في الذاكرة (حالة قراءة/عدم قراءة، حد أقصى للطرد)
  - موزّع NotificationService مع سجل تسليم، تتبع الحالة، وتدقيق عبر EventBus
  - 46 اختبار
- **nexora-graphql**: نقطة نهاية GraphQL كاملة
  - تكامل async-graphql + async-graphql-axum
  - استعلامات: me, user, users, package, packages, invoice, invoices, events, health
  - طفرات: createNotification, markNotificationRead, revokeSession
  - اشتراكات: بث الأحداث عبر EventBus
  - GraphQL Playground على GET /api/graphql
  - 8 اختبارات
- **تكامل nexora-gateway**:
  - 7 مسارات إشعارات جديدة (GET/POST/DELETE)
  - مسار GraphQL: POST /api/graphql للاستعلامات/الطفرات
  - GatewayState الآن تحمل NotificationService + مخطط GraphQL

### إحصائيات
- 18 crate (كانت 16)
- 440+ اختبار ناجح
- 0 كتلة unsafe في كل الكود الجديد

## [v1.4.0] — 2026-07-02

### أُضيف
- **nexora-wasm-sandbox** (الجزء 9): بيئة تشغيل آمنة لوحدات WASM
  - نموذج قدرة (8 قدرات: Log, ReadConfig, PublishEvent, HttpRequest, StorageRead/Write, Clock, Random)
  - حدود وقود (fuel) + ذاكرة + timeout
  - 21 اختبار
- **nexora-benchmarks**: 9 مجموعات قياس أداء
  - إطارات NXP، AEAD، Ed25519، EventBus، رموز المصادقة، المتجر، الفوترة، الإشعارات، WASM
  - 10 اختبارات + criterion harness
- **خلفية PostgreSQL** (وحدة nexora-storage pg): 7 مخازن PostgreSQL أصلية
  - PgUserStore، PgSessionStore، PgEventStore، PgPackageStore، PgBillingStore، PgAuditStore، PgSecretStore
  - PostgreSQL أصبح الخلفية الافتراضية
  - SQLite يبقى كبديل طرفي (الجزء 10)
- **SAML/OIDC SSO** (وحدة nexora-auth sso): مصادقة مؤسسية
  - OIDC Authorization Code flow مع PKCE
  - SAML 2.0 SP-initiated SSO
  - SsoSessionManager لتتبع الجلسات
  - 28 اختبار

### إحصائيات
- 16 crate (كانت 14)
- 388+ اختبار ناجح
- 0 كتلة unsafe في كل الكود الجديد

## [v1.0.0] — 2026-07-01

### أُضيف — الإصدار الأولي
- **NXP** (بروتوكول Nexora للتبادل): بروتوكول ثنائي فوق QUIC
  - nxp-core: الإطارات، الأكواد، الأخطاء
  - nxp-payload: MessagePack / CBOR
  - nxp-security: ChaCha20-Poly1305 AEAD، Ed25519، X25519 ECDHE
  - nxp-session: مصافحة HELLO، مدير الجلسة، نبضات القلب
  - nxp-transport: QUIC عبر quinn
- **nexora-core**: نواة نظام تشغيل سحابي مع 8 أنظمة فرعية
- **nexora-auth**: إدارة المستخدمين، الجلسات، الرموز (Argon2id + Ed25519)
- **nexora-gateway**: بوابة HTTP ↔ NXP (السطح HTTP الوحيد)
- **nexora-marketplace**: متجر مع 6 أنواع حزم، أمان 5 طبقات، خط أنابيب 13 خطوة
- **nexora-billing**: فواتير، مدفوعات، اشتراكات (5 نماذج فوترة)
- **nexora-storage**: تخزين SQLite الدائم (مستخدمون، أحداث، حزم، فوترة)
- **nexora-workflow**: محرك سير عمل يحركه الأحداث
- **nexora-cluster**: مدير عنقود متعدد العقد
- **واجهة SvelteKit الأمامية**: 11 صفحة (لوحة تحكم، أحداث، وحدات، صحة، إلخ)
- **9 ثنائيات عرض توضيحي** + اختبارات دخان

### الامتثال
- الجزء 3 (NXP): ✅ كامل
- الجزء 4 (Nexora Core): ✅ 8 أنظمة فرعية
- الجزء 5 (المتجر): ✅ كامل
- الجزء 6 (بنية الخلفية): ✅ البوابة
- الجزء 7 (الواجهة الأمامية): ✅ SvelteKit
- الجزء 8 (البيانات والأحداث): ✅ Event Sourcing
- الجزء 9 (الأمان): ✅ Argon2id + Ed25519
- الجزء 10 (الموارد المنخفضة): ✅ SQLite مضمّن
- الجزء 11 (AI): ✅ محجوز
- الجزء 13 (الملاحظة): ✅ tracing

### إحصائيات
- 14 crate
- 274 اختبار ناجح
- 0 كتلة unsafe
