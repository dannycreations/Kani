use super::*;

#[test]
fn parses_all_supported_schemes() {
  let cases = [
    ("http://1.2.3.4:8080", Scheme::Http, "1.2.3.4", 8080),
    ("https://5.6.7.8:443", Scheme::Https, "5.6.7.8", 443),
    (
      "socks4://9.10.11.12:1080",
      Scheme::Socks4,
      "9.10.11.12",
      1080,
    ),
    (
      "socks5://13.14.15.16:1081",
      Scheme::Socks5,
      "13.14.15.16",
      1081,
    ),
  ];

  for (line, scheme, host, port) in cases {
    let proxy = Proxy::parse(line).unwrap();
    assert_eq!(proxy.scheme, scheme);
    assert_eq!(proxy.host, host);
    assert_eq!(proxy.port, port);
  }
}

#[test]
fn rejects_malformed_lines() {
  assert!(Proxy::parse("1.2.3.4:8080").is_none());
  assert!(Proxy::parse("ftp://1.2.3.4:8080").is_none());
  assert!(Proxy::parse("http://1.2.3.4").is_none());
  assert!(Proxy::parse("http://1.2.3.4:0").is_none());
  assert!(Proxy::parse("").is_none());
}

#[test]
fn renders_url_and_endpoint() {
  let proxy = Proxy::parse("socks5://[::1]:1080").unwrap();
  assert_eq!(proxy.url(), "socks5://[::1]:1080");
  assert_eq!(proxy.endpoint(), "[::1]:1080");
}

#[test]
fn scheme_is_socks() {
  assert!(Scheme::Socks4.is_socks());
  assert!(Scheme::Socks5.is_socks());
  assert!(!Scheme::Http.is_socks());
  assert!(!Scheme::Https.is_socks());
}
