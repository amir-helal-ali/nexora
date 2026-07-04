# بوابة Nexora API
## RFC v1.0 (مسودة)

**الحالة:** مسودة
**آخر تحديث:** 2026-07-01
**مالك الوثيقة:** هندسة منصة Nexora
**التصنيف:** داخلي — مواصفة هندسية

---

## 1. الملخص

بوابة API هي **السطح HTTP الوحيد** لمنصة Nexora (حسب الجزء 6). تترجم كل
طلب HTTP إلى إرسال بأسلوب NXP ذهاباً وإياباً. JSON خارجياً، MessagePack
داخلياً (حسب القانون 15).

## 2. المسؤوليات

- **الترجمة:** HTTP JSON ↔ NXP MessagePack
- **المصادقة:** التحقق من رمز Bearer على كل مسار محمي
- **التوجيه:** تحويل المسارات إلى أكواد NXP
- **البث المباشر:** SSE و WebSocket للأحداث الحية
- **التوثيق:** توليد OpenAPI 3.0 تلقائياً
- **التحديد:** تحديد معدل الطلبات لمنع الإساءة

## 3. المسارات

### 3.1 المسارات العامة (بدون مصادقة)

| المسار | الطريقة | الوصف |
|-------|---------|--------|
| `/api/health` | GET | فحص حياة البوابة |
| `/api/openapi.json` | GET | مواصفات OpenAPI 3.0 |
| `/api/auth/login` | POST | استبدل البيانات برمز |
| `/api/auth/refresh` | POST | جدّد رمزاً |
| `/api/graphql` | POST | نفّذ استعلام GraphQL |
| `/api/graphql` | GET | صفحة GraphQL Playground |
| `/api/ws` | GET | ترقية WebSocket |

### 3.2 المسارات المحمية (تتطلب Bearer)

| المسار | الطريقة | كود NXP | الوصف |
|-------|---------|---------|--------|
| `/api/auth/logout` | POST | AUTH_LOGOUT | أبطِل الرمز |
| `/api/core/ping` | POST | PING | ذهاب-إياب عبر النواة |
| `/api/core/events` | GET | REPLAY_EVENTS | أعد تشغيل الأحداث |
| `/api/core/events` | POST | PUBLISH_EVENT | انشر حدثاً |
| `/api/core/events/stream` | GET | – (SSE) | بث أحداث مباشر |
| `/api/core/modules` | GET | – | قائمة الوحدات |
| `/api/core/health` | GET | – | صحة النواة التجميعية |
| `/api/marketplace/packages` | GET | – | قائمة الحزم |
| `/api/marketplace/packages` | POST | – | انشر حزمة |
| `/api/billing/invoices` | GET | – | قائمة الفواتير |
| `/api/billing/invoices` | POST | – | أنشئ فاتورة |
| `/api/notifications` | GET | – | قائمة الإشعارات |
| `/api/notifications` | POST | – | أرسل إشعاراً |
| `/api/notifications/:id/read` | POST | – | علّم كمقروء |
| `/api/notifications/:id` | DELETE | – | احذف إشعاراً |

## 4. المصادقة

- **الترويسة:** `Authorization: Bearer <token>`
- **الرمز:** Ed25519 موقّع، base64
- **التحقق:** كل طلب محمي يمر عبر برمجيات وسيطة
- **الاستثناء:** SSE يقبل `?token=<urlencoded>` (لأن EventSource لا يضبط ترويسات)

## 5. البث المباشر

### 5.1 SSE (Server-Sent Events)

- `/api/core/events/stream` يبث الأحداث كما تُنشر
- العميل يستخدم `EventSource` API
- اتصال طويل الأمد، إعادة اتصال تلقائية

### 5.2 WebSocket

- `/api/ws` يوفّر قناة ثنائية الاتجاه
- مصادقة عبر `?token=` في URL
- مناسب للأوامر التفاعلية

## 6. تحديد المعدل

- حد افتراضي: 100 طلب/دقيقة لكل IP
- حد مصادقة: 5 محاولات/دقيقة لكل IP
- يتجاوز الحد: 429 Too Many Requests

## 7. معالجة الأخطاء

كل الأخطاء تُرجع بصيغة JSON موحدة:

```json
{
  "ok": false,
  "error": "وصف الخطأ"
}
```

أكواد الحالة HTTP القياسية: 200، 201، 400، 401، 403، 404، 429، 500.

## 8. المراجع

- مواصفة Nexora الهندسية، الجزء 6 (بنية الخلفية)
- OpenAPI 3.0 Specification
- RFC 7231 — HTTP Semantics
