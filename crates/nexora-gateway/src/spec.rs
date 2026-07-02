//! OpenAPI 3.0 specification for the gateway.
//!
//! Generated at runtime so it always matches the actual routes. Served at
//! `GET /api/openapi.json`.

use serde_json::{json, Value};

/// Generate the OpenAPI 3.0 spec as a JSON value.
pub fn openapi_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Nexora API Gateway",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "HTTP ↔ NXP translation layer. All HTTP traffic to the Nexora platform must pass through this gateway.",
            "license": { "name": "MIT OR Apache-2.0" }
        },
        "servers": [
            { "url": "/api", "description": "Default API base" }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Ed25519-signed session token obtained from /api/auth/login"
                }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "required": ["ok", "error"],
                    "properties": {
                        "ok": { "type": "boolean", "example": false },
                        "error": { "type": "string" }
                    }
                },
                "LoginRequest": {
                    "type": "object",
                    "required": ["username", "password"],
                    "properties": {
                        "username": { "type": "string" },
                        "password": { "type": "string", "format": "password" },
                        "client": { "type": "string", "description": "Optional client identifier" }
                    }
                },
                "LoginResponse": {
                    "type": "object",
                    "required": ["token", "session_id", "user_id", "username"],
                    "properties": {
                        "token": { "type": "string" },
                        "token_expires_at_ns": { "type": "integer" },
                        "session_id": { "type": "string" },
                        "user_id": { "type": "string" },
                        "username": { "type": "string" }
                    }
                },
                "RefreshRequest": {
                    "type": "object",
                    "required": ["token"],
                    "properties": {
                        "token": { "type": "string" }
                    }
                },
                "LogoutRequest": {
                    "type": "object",
                    "required": ["token"],
                    "properties": {
                        "token": { "type": "string" },
                        "session_id": { "type": "string" }
                    }
                },
                "PublishEventRequest": {
                    "type": "object",
                    "required": ["name", "payload"],
                    "properties": {
                        "name": { "type": "string", "example": "project.created" },
                        "payload": { "type": "string" }
                    }
                }
            }
        },
        "paths": {
            "/api/health": {
                "get": {
                    "tags": ["system"],
                    "summary": "Gateway liveness probe",
                    "responses": {
                        "200": {
                            "description": "Gateway is alive",
                            "content": { "application/json": { "schema": { "type": "object" } } }
                        }
                    }
                }
            },
            "/api/openapi.json": {
                "get": {
                    "tags": ["system"],
                    "summary": "This OpenAPI spec",
                    "responses": { "200": { "description": "OpenAPI 3.0 JSON" } }
                }
            },
            "/api/auth/login": {
                "post": {
                    "tags": ["auth"],
                    "summary": "Exchange credentials for a session token",
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LoginRequest" } } }
                    },
                    "responses": {
                        "200": {
                            "description": "Login successful",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LoginResponse" } } }
                        },
                        "401": {
                            "description": "Invalid credentials",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
                        }
                    }
                }
            },
            "/api/auth/refresh": {
                "post": {
                    "tags": ["auth"],
                    "summary": "Exchange a valid token for a new one (rotates the old)",
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RefreshRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Refreshed token", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LoginResponse" } } } },
                        "401": { "description": "Token invalid or expired", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } }
                    }
                }
            },
            "/api/auth/logout": {
                "post": {
                    "tags": ["auth"],
                    "summary": "Revoke a session token",
                    "security": [{ "bearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LogoutRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Logged out" },
                        "401": { "description": "Token invalid" }
                    }
                }
            },
            "/api/core/ping": {
                "post": {
                    "tags": ["core"],
                    "summary": "Send a PING through the Core",
                    "security": [{ "bearerAuth": [] }],
                    "responses": {
                        "200": { "description": "Pong" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/core/events": {
                "get": {
                    "tags": ["core"],
                    "summary": "Replay events from the EventBus",
                    "security": [{ "bearerAuth": [] }],
                    "parameters": [
                        { "name": "from_id", "in": "query", "schema": { "type": "integer", "default": 0 } },
                        { "name": "filter", "in": "query", "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "Event list" }, "401": { "description": "Unauthorized" } }
                },
                "post": {
                    "tags": ["core"],
                    "summary": "Publish an event",
                    "security": [{ "bearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PublishEventRequest" } } }
                    },
                    "responses": { "200": { "description": "Published" }, "401": { "description": "Unauthorized" } }
                }
            },
            "/api/core/modules": {
                "get": {
                    "tags": ["core"],
                    "summary": "List installed modules",
                    "security": [{ "bearerAuth": [] }],
                    "responses": { "200": { "description": "Module list" }, "401": { "description": "Unauthorized" } }
                }
            },
            "/api/core/modules/{id}": {
                "get": {
                    "tags": ["core"],
                    "summary": "Get a single module",
                    "security": [{ "bearerAuth": [] }],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "Module" }, "404": { "description": "Not found" } }
                }
            },
            "/api/core/health": {
                "get": {
                    "tags": ["core"],
                    "summary": "Aggregate Core health",
                    "security": [{ "bearerAuth": [] }],
                    "responses": { "200": { "description": "Health snapshot" } }
                }
            }
        }
    })
}
