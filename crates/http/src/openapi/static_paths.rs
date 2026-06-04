//! Hand-written OpenAPI paths/components for the fixed (non-dynamic) routes:
//! /healthz, /auth/*, /admin/content-types*, /admin/users*, /admin/media/*.
//! These handlers are stable-shaped; update this literal if one changes.

use serde_json::{json, Value};

pub fn static_paths() -> Value {
    let secured = json!([{ "bearerAuth": [] }]);
    json!({
        "/healthz": {
            "get": {
                "tags": ["system"],
                "summary": "Liveness probe",
                "responses": { "200": { "description": "OK" } }
            }
        },
        "/auth/setup": {
            "get": {
                "tags": ["auth"],
                "summary": "First-run setup status",
                "responses": { "200": { "description": "Setup status" } }
            },
            "post": {
                "tags": ["auth"],
                "summary": "Create the first admin user",
                "requestBody": { "required": true, "content": { "application/json": {
                    "schema": { "type": "object",
                        "properties": {
                            "email": { "type": "string", "format": "email" },
                            "password": { "type": "string" }
                        },
                        "required": ["email", "password"] }
                }}},
                "responses": { "201": { "description": "Admin created" } }
            }
        },
        "/auth/login": {
            "post": {
                "tags": ["auth"],
                "summary": "Exchange credentials for a bearer token",
                "requestBody": { "required": true, "content": { "application/json": {
                    "schema": { "type": "object",
                        "properties": {
                            "email": { "type": "string", "format": "email" },
                            "password": { "type": "string" }
                        },
                        "required": ["email", "password"] }
                }}},
                "responses": {
                    "200": { "description": "Token issued", "content": { "application/json": {
                        "schema": { "type": "object", "properties": { "token": { "type": "string" } } }
                    }}},
                    "401": { "$ref": "#/components/responses/Unauthorized" }
                }
            }
        },
        "/auth/me": {
            "get": {
                "tags": ["auth"], "summary": "Current principal",
                "security": secured,
                "responses": { "200": { "description": "Principal" },
                    "401": { "$ref": "#/components/responses/Unauthorized" } }
            }
        },
        "/admin/content-types": {
            "get": { "tags": ["schema"], "summary": "List content types",
                "security": secured, "responses": { "200": { "description": "Content types" } } },
            "post": { "tags": ["schema"], "summary": "Create a content type",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/content-types/{name}": {
            "get": { "tags": ["schema"], "summary": "Fetch one content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "Content type" } } },
            "patch": { "tags": ["schema"], "summary": "Patch a content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["schema"], "summary": "Delete a content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/users": {
            "get": { "tags": ["users"], "summary": "List users",
                "security": secured, "responses": { "200": { "description": "Users" } } },
            "post": { "tags": ["users"], "summary": "Create a user",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/users/{id}": {
            "patch": { "tags": ["users"], "summary": "Update a user",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["users"], "summary": "Delete a user",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/providers": {
            "get": { "tags": ["media"], "summary": "List storage providers",
                "security": secured, "responses": { "200": { "description": "Providers" } } }
        },
        "/admin/media/settings": {
            "get": { "tags": ["media"], "summary": "Get media settings",
                "security": secured, "responses": { "200": { "description": "Settings" } } },
            "put": { "tags": ["media"], "summary": "Update media settings",
                "security": secured, "responses": { "204": { "description": "Updated" } } }
        },
        "/admin/media/settings/test": {
            "post": { "tags": ["media"], "summary": "Test provider settings",
                "security": secured, "responses": { "200": { "description": "Result" } } }
        },
        "/admin/media/folders": {
            "get": { "tags": ["media"], "summary": "List folders",
                "security": secured, "responses": { "200": { "description": "Folders" } } },
            "post": { "tags": ["media"], "summary": "Create a folder",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/media/folders/{id}": {
            "patch": { "tags": ["media"], "summary": "Update a folder",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["media"], "summary": "Delete a folder",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/assets": {
            "get": { "tags": ["media"], "summary": "List assets",
                "security": secured, "responses": { "200": { "description": "Assets" } } },
            "post": { "tags": ["media"], "summary": "Upload an asset",
                "security": secured, "responses": { "201": { "description": "Uploaded" } } }
        },
        "/admin/media/assets/{id}": {
            "get": { "tags": ["media"], "summary": "Fetch asset metadata",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Asset" } } },
            "patch": { "tags": ["media"], "summary": "Update asset metadata",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["media"], "summary": "Delete an asset",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/assets/{id}/raw": {
            "get": { "tags": ["media"], "summary": "Download raw asset bytes",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Raw bytes" } } }
        }
    })
}

/// Reusable `components.responses` referenced by both static and dynamic paths,
/// plus a shared `Error` schema.
pub fn static_components() -> Value {
    let err_content = json!({ "application/json": {
        "schema": { "$ref": "#/components/schemas/Error" }
    }});
    json!({
        "schemas": {
            "Error": {
                "type": "object",
                "properties": {
                    "error": { "type": "string" },
                    "message": { "type": "string" }
                }
            }
        },
        "responses": {
            "Unauthorized": { "description": "Missing or invalid token", "content": err_content },
            "Forbidden": { "description": "Not permitted", "content": err_content },
            "NotFound": { "description": "Resource not found", "content": err_content }
        },
        "securitySchemes": {
            "bearerAuth": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_paths_include_auth_and_admin() {
        let p = static_paths();
        assert!(p["/auth/login"]["post"].is_object());
        assert!(p["/admin/content-types"]["get"].is_object());
        assert!(p["/admin/media/assets/{id}/raw"]["get"].is_object());
    }

    #[test]
    fn components_define_error_and_security() {
        let c = static_components();
        assert!(c["schemas"]["Error"].is_object());
        assert_eq!(c["securitySchemes"]["bearerAuth"]["scheme"], "bearer");
        assert!(c["responses"]["Unauthorized"].is_object());
    }
}
