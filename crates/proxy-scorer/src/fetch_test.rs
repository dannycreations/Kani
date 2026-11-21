use super::*;
use crate::model::Scheme;

#[test]
fn parses_and_deduplicates_list() {
  let text = concat!(
    "http://1.1.1.1:80\n",
    "socks5://2.2.2.2:1080\n",
    "http://1.1.1.1:80\n",
    "garbage line\n",
    "\n",
    "socks4://3.3.3.3:4145\n",
  );
  let proxies = parse_list(text);
  assert_eq!(proxies.len(), 3);
  assert_eq!(proxies[0].host, "1.1.1.1");
  assert_eq!(proxies[1].scheme, Scheme::Socks5);
  assert_eq!(proxies[2].scheme, Scheme::Socks4);
}
