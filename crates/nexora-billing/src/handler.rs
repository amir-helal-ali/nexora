//! Billing handler — dispatches billing commands via the Gateway.

use crate::store::BillingError;
use crate::types::{Invoice, InvoiceItem, Payment, Subscription};
use crate::BillingService;
use nxp_core::NxpError;
use nxp_core::error::protocol_codes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// The Billing handler. Owns a reference to the service.
#[derive(Clone)]
pub struct BillingHandler {
    service: Arc<BillingService>,
}

impl std::fmt::Debug for BillingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingHandler")
            .field("service", &self.service)
            .finish()
    }
}

impl BillingHandler {
    /// Construct a new handler.
    pub fn new(service: Arc<BillingService>) -> Self {
        Self { service }
    }

    /// Execute a billing command.
    pub async fn execute(&self, command: &str, args: &Value) -> Result<Value, NxpError> {
        match command {
            "billing.create_invoice" => self.cmd_create_invoice(args),
            "billing.list_invoices" => self.cmd_list_invoices(),
            "billing.get_invoice" => self.cmd_get_invoice(args),
            "billing.list_customer_invoices" => self.cmd_list_customer_invoices(args),
            "billing.create_payment" => self.cmd_create_payment(args),
            "billing.succeed_payment" => self.cmd_succeed_payment(args),
            "billing.fail_payment" => self.cmd_fail_payment(args),
            "billing.list_payments" => self.cmd_list_payments(),
            "billing.create_subscription" => self.cmd_create_subscription(args),
            "billing.list_subscriptions" => self.cmd_list_subscriptions(),
            "billing.list_customer_subscriptions" => self.cmd_list_customer_subscriptions(args),
            "billing.cancel_subscription" => self.cmd_cancel_subscription(args),
            "billing.renew_subscription" => self.cmd_renew_subscription(args),
            "billing.process_renewals" => self.cmd_process_renewals(args),
            "billing.stats" => self.cmd_stats(),
            _ => Err(NxpError::protocol(
                protocol_codes::UNKNOWN_OPCODE,
                format!("unknown billing command: {}", command),
            )),
        }
    }

    fn cmd_create_invoice(&self, args: &Value) -> Result<Value, NxpError> {
        let req: CreateInvoiceRequest = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let mut invoice = Invoice::new_draft(&req.customer_id, &req.customer_name, &req.currency);
        for item in &req.items {
            invoice.add_item(InvoiceItem {
                description: item.description.clone(),
                package_id: item.package_id.clone(),
                quantity: item.quantity,
                unit_price_minor: item.unit_price_minor,
                total_minor: item.quantity * item.unit_price_minor,
                currency: req.currency.clone(),
            });
        }
        invoice.mark_open();
        let inv = self
            .service
            .store
            .create_invoice(invoice)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "invoice": inv }))
    }

    fn cmd_list_invoices(&self) -> Result<Value, NxpError> {
        let invoices = self.service.store.list_invoices();
        Ok(serde_json::json!({ "ok": true, "count": invoices.len(), "invoices": invoices }))
    }

    fn cmd_get_invoice(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let inv = self
            .service
            .store
            .get_invoice(id)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, format!("invoice {} not found", id)))?;
        Ok(serde_json::json!({ "ok": true, "invoice": inv }))
    }

    fn cmd_list_customer_invoices(&self, args: &Value) -> Result<Value, NxpError> {
        let customer_id = args
            .get("customer_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing customer_id"))?;
        let invoices = self.service.store.list_invoices_for_customer(customer_id);
        Ok(serde_json::json!({ "ok": true, "count": invoices.len(), "invoices": invoices }))
    }

    fn cmd_create_payment(&self, args: &Value) -> Result<Value, NxpError> {
        let req: CreatePaymentRequest = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let invoice = self
            .service
            .store
            .get_invoice(&req.invoice_id)
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "invoice not found"))?;
        let payment = Payment::new_pending(
            &req.invoice_id,
            &invoice.customer_id,
            invoice.total_minor,
            &invoice.currency,
            &req.method,
        );
        let pay = self
            .service
            .store
            .create_payment(payment)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "payment": pay }))
    }

    fn cmd_succeed_payment(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let (pay, inv) = self
            .service
            .store
            .succeed_payment(id)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "payment": pay, "invoice": inv }))
    }

    fn cmd_fail_payment(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let pay = self
            .service
            .store
            .fail_payment(id, reason)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "payment": pay }))
    }

    fn cmd_list_payments(&self) -> Result<Value, NxpError> {
        let payments = self.service.store.list_payments();
        Ok(serde_json::json!({ "ok": true, "count": payments.len(), "payments": payments }))
    }

    fn cmd_create_subscription(&self, args: &Value) -> Result<Value, NxpError> {
        let req: CreateSubscriptionRequest = serde_json::from_value(args.clone())
            .map_err(|e| NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string()))?;
        let sub = Subscription::new_active(
            &req.customer_id,
            &req.package_id,
            req.price_minor,
            &req.currency,
            req.period_seconds,
        );
        // Generate the first invoice immediately.
        let invoice = sub.generate_invoice(&req.customer_name);
        let sub = self
            .service
            .store
            .create_subscription(sub)
            .map_err(map_billing_error)?;
        let inv = self
            .service
            .store
            .create_invoice(invoice)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "subscription": sub, "invoice": inv }))
    }

    fn cmd_list_subscriptions(&self) -> Result<Value, NxpError> {
        let subs = self.service.store.list_subscriptions();
        Ok(serde_json::json!({ "ok": true, "count": subs.len(), "subscriptions": subs }))
    }

    fn cmd_list_customer_subscriptions(&self, args: &Value) -> Result<Value, NxpError> {
        let customer_id = args
            .get("customer_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing customer_id"))?;
        let subs = self.service.store.list_subscriptions_for_customer(customer_id);
        Ok(serde_json::json!({ "ok": true, "count": subs.len(), "subscriptions": subs }))
    }

    fn cmd_cancel_subscription(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let sub = self
            .service
            .store
            .cancel_subscription(id)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "subscription": sub }))
    }

    fn cmd_renew_subscription(&self, args: &Value) -> Result<Value, NxpError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NxpError::protocol(protocol_codes::DECODE_FAILED, "missing id"))?;
        let name = args
            .get("customer_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Customer");
        let (sub, inv) = self
            .service
            .store
            .renew_subscription(id, name)
            .map_err(map_billing_error)?;
        Ok(serde_json::json!({ "ok": true, "subscription": sub, "invoice": inv }))
    }

    fn cmd_process_renewals(&self, _args: &Value) -> Result<Value, NxpError> {
        let renewals = self.service.store.process_renewals(|_| "Customer".into());
        Ok(serde_json::json!({
            "ok": true,
            "renewed_count": renewals.len(),
            "renewals": renewals.iter().map(|(s, i)| {
                serde_json::json!({
                    "subscription_id": s.id,
                    "invoice_id": i.id,
                    "amount_minor": i.total_minor,
                    "currency": i.currency,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    fn cmd_stats(&self) -> Result<Value, NxpError> {
        let invoices = self.service.store.list_invoices();
        let paid: u64 = invoices
            .iter()
            .filter(|i| i.status == crate::types::InvoiceStatus::Paid)
            .map(|i| i.total_minor)
            .sum();
        let outstanding: u64 = invoices
            .iter()
            .filter(|i| {
                i.status == crate::types::InvoiceStatus::Open || i.status == crate::types::InvoiceStatus::PastDue
            })
            .map(|i| i.total_minor)
            .sum();
        Ok(serde_json::json!({
            "ok": true,
            "stats": {
                "invoice_count": self.service.store.invoice_count(),
                "payment_count": self.service.store.payment_count(),
                "subscription_count": self.service.store.subscription_count(),
                "revenue_minor": paid,
                "outstanding_minor": outstanding,
                "currency": if invoices.is_empty() { "USD".to_string() } else { invoices[0].currency.clone() },
            }
        }))
    }
}

// ---- Request types ----

#[derive(Debug, Deserialize, Serialize)]
struct CreateInvoiceRequest {
    customer_id: String,
    customer_name: String,
    currency: String,
    items: Vec<CreateInvoiceItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateInvoiceItem {
    description: String,
    #[serde(default)]
    package_id: Option<String>,
    quantity: u64,
    unit_price_minor: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CreatePaymentRequest {
    invoice_id: String,
    method: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateSubscriptionRequest {
    customer_id: String,
    customer_name: String,
    package_id: String,
    price_minor: u64,
    currency: String,
    period_seconds: u64,
}

fn map_billing_error(e: BillingError) -> NxpError {
    NxpError::protocol(protocol_codes::DECODE_FAILED, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexora_core::NexoraCore;

    fn setup() -> BillingHandler {
        let core = Arc::new(NexoraCore::new());
        let svc = Arc::new(BillingService::new(core));
        BillingHandler::new(svc)
    }

    #[tokio::test]
    async fn create_invoice_and_list() {
        let h = setup();
        let req = serde_json::json!({
            "customer_id": "u1",
            "customer_name": "Alice",
            "currency": "USD",
            "items": [
                { "description": "Test", "quantity": 2, "unit_price_minor": 500 }
            ]
        });
        let resp = h.execute("billing.create_invoice", &req).await.unwrap();
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["invoice"]["total_minor"], 1000);

        let resp = h.execute("billing.list_invoices", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["count"], 1);
    }

    #[tokio::test]
    async fn create_payment_and_succeed() {
        let h = setup();
        // Create invoice.
        let inv_resp = h
            .execute(
                "billing.create_invoice",
                &serde_json::json!({
                    "customer_id": "u1",
                    "customer_name": "Alice",
                    "currency": "USD",
                    "items": [{ "description": "Test", "quantity": 1, "unit_price_minor": 999 }]
                }),
            )
            .await
            .unwrap();
        let inv_id = inv_resp["invoice"]["id"].as_str().unwrap().to_string();

        // Create payment.
        let pay_resp = h
            .execute(
                "billing.create_payment",
                &serde_json::json!({ "invoice_id": inv_id, "method": "card" }),
            )
            .await
            .unwrap();
        let pay_id = pay_resp["payment"]["id"].as_str().unwrap().to_string();

        // Succeed payment.
        let resp = h
            .execute("billing.succeed_payment", &serde_json::json!({ "id": pay_id }))
            .await
            .unwrap();
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["payment"]["status"], "succeeded");
        assert_eq!(resp["invoice"]["status"], "paid");
    }

    #[tokio::test]
    async fn subscription_creates_first_invoice() {
        let h = setup();
        let resp = h
            .execute(
                "billing.create_subscription",
                &serde_json::json!({
                    "customer_id": "u1",
                    "customer_name": "Alice",
                    "package_id": "com.test.pkg",
                    "price_minor": 999,
                    "currency": "USD",
                    "period_seconds": 2592000
                }),
            )
            .await
            .unwrap();
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["subscription"]["status"], "active");
        assert_eq!(resp["invoice"]["total_minor"], 999);
    }

    #[tokio::test]
    async fn stats_works() {
        let h = setup();
        h.execute(
            "billing.create_invoice",
            &serde_json::json!({
                "customer_id": "u1",
                "customer_name": "Alice",
                "currency": "USD",
                "items": [{ "description": "Test", "quantity": 1, "unit_price_minor": 1000 }]
            }),
        )
        .await
        .unwrap();
        let resp = h.execute("billing.stats", &serde_json::json!({})).await.unwrap();
        assert_eq!(resp["stats"]["invoice_count"], 1);
        assert_eq!(resp["stats"]["outstanding_minor"], 1000);
        assert_eq!(resp["stats"]["revenue_minor"], 0);
    }

    #[tokio::test]
    async fn unknown_command_rejected() {
        let h = setup();
        let err = h.execute("billing.nope", &serde_json::json!({})).await.unwrap_err();
        assert_eq!(err.code, protocol_codes::UNKNOWN_OPCODE);
    }
}
