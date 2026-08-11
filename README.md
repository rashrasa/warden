# Warden

An API gateway written in Rust.

## Configuration

### Example

```json
{
  "identity": {
    "jwt-default": {
      "jwt": { "secret": "!env WARDEN_JWT_SIGNING_SECRET" }
    }
  },
  "roles": {
    "user1": {
      "identity": ["jwt-default"]
    }
  },
  "handlers": {
    "/": {
      "protocol": "html",
      "path": "./warden-core/assets/hello.html",
      "cache": "static",
      "permission": {
        "type": "block",
        "roles": []
      }
    },
    "/dyn": {
      "protocol": "html",
      "path": "./warden-core/assets/dynamic.html",
      "cache": "none",
      "permission": {
        "type": "allow",
        "roles": ["user1"]
      }
    }
  }
}
```