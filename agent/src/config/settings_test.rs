#[cfg(test)]
mod tests {
    use crate::config::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();

        assert_eq!(settings.server.host, "127.0.0.1");
        assert_eq!(settings.server.port, 9718);
        assert_eq!(settings.security.allowed_origins, vec!["*".to_string()]);
        assert!(settings.security.auth_token.is_none());
        assert!(settings.security.https_only);
        assert!(settings.printing.temp_directory.is_none());
        assert_eq!(settings.printing.cleanup_interval, 3600);
        assert_eq!(settings.printing.max_cache_age, 86400);
        assert_eq!(settings.logging.level, "info");
    }

    #[test]
    fn test_origin_wildcard() {
        let settings = Settings::default();

        // Wildcard should allow everything
        assert!(settings.is_origin_allowed("https://example.com"));
        assert!(settings.is_origin_allowed("http://localhost:3000"));
        assert!(settings.is_origin_allowed("https://app.mysite.com"));
        assert!(settings.is_origin_allowed("file://"));
    }

    #[test]
    fn test_origin_exact_match() {
        let mut settings = Settings::default();
        settings.security.allowed_origins = vec![
            "https://example.com".to_string(),
            "https://app.example.com".to_string(),
        ];

        assert!(settings.is_origin_allowed("https://example.com"));
        assert!(settings.is_origin_allowed("https://app.example.com"));
        assert!(!settings.is_origin_allowed("https://other.com"));
        assert!(!settings.is_origin_allowed("http://example.com")); // Different scheme
    }

    #[test]
    fn test_origin_subdomain_wildcard() {
        let mut settings = Settings::default();
        settings.security.allowed_origins = vec!["https://*.example.com".to_string()];

        assert!(settings.is_origin_allowed("https://app.example.com"));
        assert!(settings.is_origin_allowed("https://api.example.com"));
        assert!(settings.is_origin_allowed("https://staging.example.com"));
        assert!(!settings.is_origin_allowed("https://example.com")); // No subdomain
        assert!(!settings.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn test_origin_simple_wildcard() {
        let mut settings = Settings::default();
        settings.security.allowed_origins = vec!["*.example.com".to_string()];

        assert!(settings.is_origin_allowed("app.example.com"));
        assert!(settings.is_origin_allowed("api.example.com"));
    }

    #[test]
    fn test_auth_no_token_required() {
        let settings = Settings::default();

        // No auth token set, should allow anything
        assert!(settings.check_auth(None));
        assert!(settings.check_auth(Some("random")));
        assert!(settings.check_auth(Some("")));
    }

    #[test]
    fn test_auth_token_required() {
        let mut settings = Settings::default();
        settings.security.auth_token = Some("secret-token-123".to_string());

        assert!(!settings.check_auth(None));
        assert!(!settings.check_auth(Some("")));
        assert!(!settings.check_auth(Some("wrong-token")));
        assert!(settings.check_auth(Some("secret-token-123")));
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();

        assert!(json.contains("\"host\": \"127.0.0.1\""));
        assert!(json.contains("\"port\": 9718"));
        assert!(json.contains("\"allowedOrigins\""));

        // Deserialize back
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server.host, settings.server.host);
        assert_eq!(parsed.server.port, settings.server.port);
    }

    #[test]
    fn test_settings_partial_json() {
        // Test that missing fields use defaults
        let json = r#"{
            "server": {
                "port": 9000
            }
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.server.port, 9000);
        assert_eq!(settings.server.host, "127.0.0.1"); // Default
        assert_eq!(settings.security.allowed_origins, vec!["*".to_string()]); // Default
    }

    #[test]
    fn test_server_settings_custom() {
        let json = r#"{
            "server": {
                "host": "0.0.0.0",
                "port": 8080
            },
            "security": {
                "allowedOrigins": ["https://myapp.com"],
                "authToken": "my-secret",
                "httpsOnly": false
            }
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.server.host, "0.0.0.0");
        assert_eq!(settings.server.port, 8080);
        assert_eq!(settings.security.allowed_origins, vec!["https://myapp.com"]);
        assert_eq!(settings.security.auth_token, Some("my-secret".to_string()));
        assert!(!settings.security.https_only);
    }

    #[test]
    fn test_printing_settings() {
        let json = r#"{
            "printing": {
                "tempDirectory": "/custom/path",
                "cleanupInterval": 7200,
                "maxCacheAge": 172800,
                "defaultPrinter": "My Printer"
            }
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(
            settings.printing.temp_directory,
            Some(std::path::PathBuf::from("/custom/path"))
        );
        assert_eq!(settings.printing.cleanup_interval, 7200);
        assert_eq!(settings.printing.max_cache_age, 172800);
        assert_eq!(settings.printing.default_printer, Some("My Printer".to_string()));
    }

    #[test]
    fn test_logging_settings() {
        let json = r#"{
            "logging": {
                "level": "debug",
                "maxFiles": 10,
                "maxSize": "50MB"
            }
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.logging.level, "debug");
        assert_eq!(settings.logging.max_files, 10);
        assert_eq!(settings.logging.max_size, "50MB");
    }

    #[test]
    fn test_multiple_origins() {
        let mut settings = Settings::default();
        settings.security.allowed_origins = vec![
            "https://app1.com".to_string(),
            "https://app2.com".to_string(),
            "https://*.internal.com".to_string(),
        ];

        assert!(settings.is_origin_allowed("https://app1.com"));
        assert!(settings.is_origin_allowed("https://app2.com"));
        assert!(settings.is_origin_allowed("https://dev.internal.com"));
        assert!(!settings.is_origin_allowed("https://app3.com"));
    }
}
