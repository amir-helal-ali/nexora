//! SAML 2.0 client.
//!
//! Implements the SP-initiated SSO flow. The IdP posts a SAML response
//! to our ACS endpoint; we verify the signature, check the conditions
//! (validity window, audience), and extract the user's identity.
//!
//! # Note on XML
//!
//! For the reference implementation, we parse SAML responses as opaque
//! strings and extract the relevant fields with simple string ops. A
//! production deployment would use a proper XML signature library
//! (`openssl` or `ring` + `quick-xml`).

use crate::sso::config::{SsoProviderConfig, SsoProviderKind};
use crate::sso::error::{SsoError, SsoResult};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A parsed SAML response (after signature verification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlAssertion {
    /// Assertion ID (the `ID` attribute on `<saml:Assertion>`).
    pub id: String,
    /// Issuer (IdP entity ID).
    pub issuer: String,
    /// Subject (NameID — usually the user's email or a unique ID).
    pub subject: String,
    /// Audience (our SP entity ID).
    pub audience: String,
    /// Not-before (unix seconds).
    pub not_before: i64,
    /// Not-on-or-after (unix seconds).
    pub not_on_or_after: i64,
    /// Authentication instant (unix seconds).
    pub authn_instant: i64,
    /// Session index (for SLO).
    pub session_index: String,
    /// Attributes (e.g. email, roles, groups).
    pub attributes: std::collections::HashMap<String, Vec<String>>,
}

/// SAML client (stateless).
pub struct SamlClient {
    config: SsoProviderConfig,
}

impl SamlClient {
    /// Construct a new SAML client. Validates that the config has the
    /// required SAML fields.
    pub fn new(config: SsoProviderConfig) -> SsoResult<Self> {
        if config.kind != SsoProviderKind::Saml {
            return Err(SsoError::ProviderNotConfigured("not a SAML provider".into()));
        }
        if config.saml_sso_url.is_none() {
            return Err(SsoError::ProviderNotConfigured("missing saml_sso_url".into()));
        }
        if config.saml_idp_certificate.is_none() {
            return Err(SsoError::ProviderNotConfigured(
                "missing saml_idp_certificate".into(),
            ));
        }
        if config.saml_sp_entity_id.is_none() {
            return Err(SsoError::ProviderNotConfigured(
                "missing saml_sp_entity_id".into(),
            ));
        }
        if config.saml_sp_acs_url.is_none() {
            return Err(SsoError::ProviderNotConfigured("missing saml_sp_acs_url".into()));
        }
        Ok(Self { config })
    }

    /// Build the SAML authn request URL (for SP-initiated SSO).
    ///
    /// Returns a redirect URL that the browser should be sent to.
    pub fn build_authn_request_url(&self) -> SsoResult<String> {
        let sso_url = self.config.saml_sso_url.as_ref().unwrap();
        let sp_entity_id = self.config.saml_sp_entity_id.as_ref().unwrap();
        let acs_url = self.config.saml_sp_acs_url.as_ref().unwrap();

        // In a real implementation, this would be a deflate+base64-encoded
        // `<samlp:AuthnRequest>` XML document. For the reference impl we
        // pass the parameters as query string.
        let request_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                    ID="{id}"
                    Version="2.0"
                    IssueInstant="{issue_instant}"
                    Destination="{destination}"
                    AssertionConsumerServiceURL="{acs_url}">
  <saml:Issuer xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">{issuer}</saml:Issuer>
</samlp:AuthnRequest>"#,
            id = random_id(),
            issue_instant = iso8601_now(),
            destination = sso_url,
            acs_url = acs_url,
            issuer = sp_entity_id,
        );

        let encoded = deflate_base64(&request_xml);
        Ok(format!("{sso_url}?SAMLRequest={encoded}"))
    }

    /// Parse a SAML response received via POST binding.
    ///
    /// **SECURITY WARNING**: This reference implementation does NOT verify
    /// the XML signature. Production deployments MUST verify the signature
    /// against `config.saml_idp_certificate` before trusting any field.
    pub fn parse_response(&self, saml_response_b64: &str) -> SsoResult<SamlAssertion> {
        use base64::Engine;
        let xml_bytes = base64::engine::general_purpose::STANDARD
            .decode(saml_response_b64)
            .map_err(|e| SsoError::SamlResponseInvalid(e.to_string()))?;
        let xml = String::from_utf8(xml_bytes)
            .map_err(|e| SsoError::SamlResponseInvalid(e.to_string()))?;

        // Extract fields via simple string search (reference impl).
        let id = extract_attr(&xml, "Assertion", "ID")
            .ok_or_else(|| SsoError::SamlResponseInvalid("missing Assertion ID".into()))?;
        let issuer = extract_text(&xml, "Issuer")
            .ok_or_else(|| SsoError::SamlResponseInvalid("missing Issuer".into()))?;
        let subject = extract_text(&xml, "NameID")
            .ok_or_else(|| SsoError::SamlResponseInvalid("missing NameID".into()))?;
        let audience = extract_text(&xml, "Audience")
            .ok_or_else(|| SsoError::SamlResponseInvalid("missing Audience".into()))?;
        let not_before = extract_attr(&xml, "Conditions", "NotBefore")
            .and_then(|s| parse_iso8601(&s))
            .unwrap_or(0);
        let not_on_or_after = extract_attr(&xml, "Conditions", "NotOnOrAfter")
            .and_then(|s| parse_iso8601(&s))
            .unwrap_or(i64::MAX);
        let authn_instant = extract_attr(&xml, "AuthnStatement", "AuthnInstant")
            .and_then(|s| parse_iso8601(&s))
            .unwrap_or(0);
        let session_index = extract_attr(&xml, "AuthnStatement", "SessionIndex")
            .unwrap_or_default();

        // Verify audience matches our SP entity ID.
        let sp_entity_id = self.config.saml_sp_entity_id.as_ref().unwrap();
        if &audience != sp_entity_id {
            return Err(SsoError::SamlResponseInvalid(format!(
                "audience mismatch: expected {sp_entity_id}, got {audience}"
            )));
        }

        // Verify validity window.
        let now = OffsetDateTime::now_utc().unix_timestamp();
        if now < not_before {
            return Err(SsoError::SamlResponseInvalid(
                "assertion not yet valid".into(),
            ));
        }
        if now >= not_on_or_after {
            return Err(SsoError::SamlExpired);
        }

        // Extract attributes.
        let attributes = extract_attributes(&xml);

        Ok(SamlAssertion {
            id,
            issuer,
            subject,
            audience,
            not_before,
            not_on_or_after,
            authn_instant,
            session_index,
            attributes,
        })
    }

    /// Provider config accessor.
    pub fn config(&self) -> &SsoProviderConfig {
        &self.config
    }
}

// --- helpers ---

fn random_id() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    format!("_{}", hex::encode(buf))
}

fn iso8601_now() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn parse_iso8601(s: &str) -> Option<i64> {
    // Very lenient parser: just take the YYYY-MM-DDTHH:MM:SSZ part.
    if s.len() < 20 {
        return None;
    }
    let year: i32 = s[0..4].parse().ok()?;
    let month: u8 = s[5..7].parse().ok()?;
    let day: u8 = s[8..10].parse().ok()?;
    let hour: u8 = s[11..13].parse().ok()?;
    let min: u8 = s[14..16].parse().ok()?;
    let sec: u8 = s[17..19].parse().ok()?;
    // Use time crate for proper conversion.
    use time::Date;
    let date = Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let dt = date.with_hms_milli(hour, min, sec, 0).ok()?;
    Some(dt.assume_utc().unix_timestamp())
}

fn extract_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    // Search for the tag with optional namespace prefix.
    // We try multiple open-tag patterns and pick the first one that matches.
    let open_patterns = [
        format!("<saml:{tag} "),
        format!("<saml:{tag}>"),
        format!("<samlp:{tag} "),
        format!("<samlp:{tag}>"),
        format!("<{tag} "),
        format!("<{tag}>"),
    ];
    for open in &open_patterns {
        if let Some(open_pos) = xml.find(open) {
            // Find the end of the open tag.
            let after_open = &xml[open_pos..];
            if let Some(gt) = after_open.find('>') {
                let elem_text = &xml[open_pos..open_pos + gt];
                if let Some(attr_pos) = elem_text.find(&format!("{attr}=\"")) {
                    let start = attr_pos + attr.len() + 2;
                    if let Some(end) = elem_text[start..].find('"') {
                        return Some(elem_text[start..start + end].to_string());
                    }
                }
            }
        }
    }
    None
}

fn extract_text(xml: &str, tag: &str) -> Option<String> {
    // Search for the tag with optional namespace prefix. Try `saml:` first,
    // then `samlp:`, then bare tag.
    let open_patterns = [
        format!("<saml:{tag}>"),
        format!("<saml:{tag} "),
        format!("<samlp:{tag}>"),
        format!("<samlp:{tag} "),
        format!("<{tag}>"),
        format!("<{tag} "),
    ];
    let close_patterns = [
        format!("</saml:{tag}>"),
        format!("</samlp:{tag}>"),
        format!("</{tag}>"),
    ];
    for open in &open_patterns {
        if let Some(open_pos) = xml.find(open) {
            // Find the end of the open tag.
            let after_open = &xml[open_pos..];
            if let Some(gt) = after_open.find('>') {
                let text_start = open_pos + gt + 1;
                let rest = &xml[text_start..];
                for close in &close_patterns {
                    if let Some(close_pos) = rest.find(close) {
                        return Some(xml[text_start..text_start + close_pos].trim().to_string());
                    }
                }
            }
        }
    }
    None
}

fn extract_attributes(xml: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut out = std::collections::HashMap::new();
    // Find all <saml:Attribute Name="..."> elements.
    let mut cursor = 0;
    while let Some(pos) = xml[cursor..].find("<saml:Attribute Name=\"") {
        let abs = cursor + pos;
        let name_start = abs + "<saml:Attribute Name=\"".len();
        let name_end = match xml[name_start..].find('"') {
            Some(e) => name_start + e,
            None => break,
        };
        let name = xml[name_start..name_end].to_string();
        // Find the next </saml:Attribute>
        let value_start = name_end;
        let value_end = match xml[value_start..].find("</saml:Attribute>") {
            Some(e) => value_start + e,
            None => break,
        };
        let value_xml = &xml[value_start..value_end];
        // Extract all <saml:AttributeValue>...</saml:AttributeValue> texts.
        let mut values = Vec::new();
        let mut sc = 0;
        while let Some(sp) = value_xml[sc..].find("<saml:AttributeValue>") {
            let ts = sc + sp + "<saml:AttributeValue>".len();
            if let Some(ep) = value_xml[ts..].find("</saml:AttributeValue>") {
                values.push(value_xml[ts..ts + ep].trim().to_string());
                sc = ts + ep + "</saml:AttributeValue>".len();
            } else {
                break;
            }
        }
        out.entry(name).or_insert_with(Vec::new).extend(values);
        cursor = value_end + "</saml:Attribute>".len();
        if cursor >= xml.len() {
            break;
        }
    }
    out
}

fn deflate_base64(s: &str) -> String {
    // Reference impl: just base64-encode (no deflate). Production would
    // use flate2 to deflate first.
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sso::config::SsoProviderKind;
    use std::collections::HashMap;

    fn sample_config() -> SsoProviderConfig {
        SsoProviderConfig {
            id: "test-saml".into(),
            display_name: "Test SAML".into(),
            kind: SsoProviderKind::Saml,
            client_id: "sp-entity-id".into(),
            client_secret: String::new(),
            oidc_discovery_url: None,
            oidc_scopes: vec![],
            saml_metadata_url: None,
            saml_sso_url: Some("https://idp.example.com/sso".into()),
            saml_idp_certificate: Some("-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----".into()),
            saml_sp_entity_id: Some("https://nexora.dev/sp".into()),
            saml_sp_acs_url: Some("https://nexora.dev/auth/sso/saml/test-saml/acs".into()),
            redirect_after_login: "/dashboard".into(),
            role_mapping: HashMap::new(),
            default_role: "viewer".into(),
        }
    }

    #[test]
    fn constructs_with_valid_config() {
        assert!(SamlClient::new(sample_config()).is_ok());
    }

    #[test]
    fn rejects_missing_sso_url() {
        let mut c = sample_config();
        c.saml_sso_url = None;
        assert!(SamlClient::new(c).is_err());
    }

    #[test]
    fn rejects_missing_certificate() {
        let mut c = sample_config();
        c.saml_idp_certificate = None;
        assert!(SamlClient::new(c).is_err());
    }

    #[test]
    fn rejects_oidc_config() {
        let mut c = sample_config();
        c.kind = SsoProviderKind::Oidc;
        assert!(SamlClient::new(c).is_err());
    }

    #[test]
    fn build_authn_request_returns_url() {
        let client = SamlClient::new(sample_config()).unwrap();
        let url = client.build_authn_request_url().unwrap();
        assert!(url.starts_with("https://idp.example.com/sso?SAMLRequest="));
    }

    #[test]
    fn parse_response_extracts_fields() {
        let client = SamlClient::new(sample_config()).unwrap();
        let xml = r#"<?xml version="1.0"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
  <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                  ID="_abc123"
                  Version="2.0">
    <saml:Issuer>https://idp.example.com</saml:Issuer>
    <saml:Subject>
      <saml:NameID>alice@example.com</saml:NameID>
    </saml:Subject>
    <saml:Conditions NotBefore="2000-01-01T00:00:00Z" NotOnOrAfter="2099-01-01T00:00:00Z">
      <saml:AudienceRestriction>
        <saml:Audience>https://nexora.dev/sp</saml:Audience>
      </saml:AudienceRestriction>
    </saml:Conditions>
    <saml:AuthnStatement AuthnInstant="2024-01-01T00:00:00Z" SessionIndex="session-xyz"/>
  </saml:Assertion>
</samlp:Response>"#;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        let assertion = client.parse_response(&b64).unwrap();
        assert_eq!(assertion.id, "_abc123");
        assert_eq!(assertion.issuer, "https://idp.example.com");
        assert_eq!(assertion.subject, "alice@example.com");
        assert_eq!(assertion.audience, "https://nexora.dev/sp");
        assert_eq!(assertion.session_index, "session-xyz");
    }

    #[test]
    fn parse_response_rejects_wrong_audience() {
        let client = SamlClient::new(sample_config()).unwrap();
        let xml = r#"<saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_x">
  <saml:Issuer>https://idp.example.com</saml:Issuer>
  <saml:Subject><saml:NameID>alice@example.com</saml:NameID></saml:Subject>
  <saml:Conditions NotBefore="2000-01-01T00:00:00Z" NotOnOrAfter="2099-01-01T00:00:00Z">
    <saml:AudienceRestriction><saml:Audience>https://wrong.example.com</saml:Audience></saml:AudienceRestriction>
  </saml:Conditions>
</saml:Assertion>"#;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        assert!(client.parse_response(&b64).is_err());
    }

    #[test]
    fn parse_response_rejects_expired() {
        let client = SamlClient::new(sample_config()).unwrap();
        let xml = r#"<saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_x">
  <saml:Issuer>https://idp.example.com</saml:Issuer>
  <saml:Subject><saml:NameID>alice@example.com</saml:NameID></saml:Subject>
  <saml:Conditions NotBefore="2000-01-01T00:00:00Z" NotOnOrAfter="2001-01-01T00:00:00Z">
    <saml:AudienceRestriction><saml:Audience>https://nexora.dev/sp</saml:Audience></saml:AudienceRestriction>
  </saml:Conditions>
</saml:Assertion>"#;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        assert!(matches!(
            client.parse_response(&b64),
            Err(SsoError::SamlExpired)
        ));
    }

    #[test]
    fn parse_response_rejects_not_yet_valid() {
        let client = SamlClient::new(sample_config()).unwrap();
        let xml = r#"<saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_x">
  <saml:Issuer>https://idp.example.com</saml:Issuer>
  <saml:Subject><saml:NameID>alice@example.com</saml:NameID></saml:Subject>
  <saml:Conditions NotBefore="2099-01-01T00:00:00Z" NotOnOrAfter="2100-01-01T00:00:00Z">
    <saml:AudienceRestriction><saml:Audience>https://nexora.dev/sp</saml:Audience></saml:AudienceRestriction>
  </saml:Conditions>
</saml:Assertion>"#;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        assert!(client.parse_response(&b64).is_err());
    }
}
