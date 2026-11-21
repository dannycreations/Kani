use super::*;

#[test]
fn parses_host_and_port() {
  let endpoint = "1.2.3.4:8080".parse::<Endpoint>().unwrap();
  assert_eq!(endpoint.host, "1.2.3.4");
  assert_eq!(endpoint.port, 8080);
}

#[test]
fn parses_scheme_prefixed_endpoint() {
  let endpoint = "socks5://proxy.example:1080/".parse::<Endpoint>().unwrap();
  assert_eq!(endpoint.host, "proxy.example");
  assert_eq!(endpoint.port, 1080);
}

#[test]
fn parses_ipv6_endpoint() {
  let endpoint = "[::1]:3128".parse::<Endpoint>().unwrap();
  assert_eq!(endpoint.host, "::1");
  assert_eq!(endpoint.socket_address(), "[::1]:3128");
}

#[test]
fn rejects_invalid_endpoints() {
  assert!("host".parse::<Endpoint>().is_err());
  assert!("host:0".parse::<Endpoint>().is_err());
  assert!("host:notaport".parse::<Endpoint>().is_err());
}

#[test]
fn proxy_string_round_trips() {
  let config = ProxyConfig {
    http: Some("1.1.1.1:80".parse().unwrap()),
    https: Some("2.2.2.2:443".parse().unwrap()),
    socks: Some("3.3.3.3:1080".parse().unwrap()),
  };
  let encoded = config.to_proxy_string();
  assert_eq!(ProxyConfig::from_proxy_string(&encoded), config);
}

#[test]
fn proxy_string_accepts_plain_address() {
  let config = ProxyConfig::from_proxy_string("proxy.local:3128");
  assert_eq!(config.http.as_ref().unwrap().host, "proxy.local");
  assert_eq!(config.https.as_ref().unwrap().port, 3128);
}

#[test]
fn empty_config_has_no_env_vars() {
  assert!(ProxyConfig::default().to_env_vars().is_empty());
}
