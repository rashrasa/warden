# Warden

An API gateway written in Rust.

## Configuration

### Example

```json
{
  "$schema": "../warden-core/assets/config-schema.json",
  "host": "0.0.0.0",
  "port": 3000,
  "tls": {
    "certs": "temp/server.crt",
    "key": "temp/server.key"
  },
  "handlers": {
    "/": {
      "protocol": "html",
      "path": "./warden-core/assets/hello.html",
      "cache": "static",
      "permission": {
        "filter": "not_equals",
        "field": {
          "jwt_claim": {
            "provider": "default",
            "key": "user"
          }
        },
        "any": ["blocked_user1", "blocked_user2", "blocked_user3"]
      }
    },
    "/dyn": {
      "protocol": "html",
      "path": "./warden-core/assets/dynamic.html",
      "cache": "none",
      "permission": {
        "filter": "equals",
        "field": {
          "jwt_claim": {
            "provider": "default",
            "key": "user"
          }
        },
        "value": "service_role"
      }
    }
  },
  "providers": {
    "default": {
      "jwt": {
        "public_key_pem": "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAwmK6SSAu2E9V7uynkCKEaj5nZJyTvNG4x0KohsRzLpg=\n-----END PUBLIC KEY-----"
      }
    }
  }
}

```