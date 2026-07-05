# سجل التغييرات

جميع التغييرات الجديرة بالملاحظة في مشروع Nexora سيتم توثيقها في هذا الملف.

التنسيق مبني على [احتفظ بسجل التغييرات](https://keepachangelog.com/ar-1.0.0/)
وهذا المشروع يلتزم بـ [الإصدار الدلالي](https://semver.org/lang/ar/).

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
