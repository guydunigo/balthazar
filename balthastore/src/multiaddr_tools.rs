use multiaddr::{AddrComponent, Multiaddr};
use std::{error::Error, fmt};

/// Try to convert a multiaddress into a usual [`String`].
/// The multiaddress must correpsond to an usual *internet* address.
///
/// As first protocol parts (**address**) are supported:
/// - `/ip4/`
/// - `/ip6/`
/// - `/dns4/`
/// - `/dns6/` // TODO: resolve here to ensure 4 or 6 conservation ?
///
/// A second protocol part (usually **port**) isn't required, but if it is provided,
/// here are those supported:
/// As second protocol parts (**ports**) are supported:
/// - `/tcp/`
/// - `/udp/`
///
/// The rest of the parts won't be inspected.
///
/// ## Example:
/// ```rust
/// # use balthastore::try_internet_multiaddr_to_usual_format;
///
/// let multiaddr_0 = "/ip4/127.0.0.1/tcp/3000".parse().unwrap();
/// let result_0 = try_internet_multiaddr_to_usual_format(&multiaddr_0);
/// assert_eq!(result_0, Ok("127.0.0.1:3000".to_string()));
///
/// let multiaddr_1 = "/dns6/rust-lang.org".parse().unwrap();
/// let result_1 = try_internet_multiaddr_to_usual_format(&multiaddr_1);
/// assert_eq!(result_1, Ok("rust-lang.org".to_string()));
/// ```
pub fn try_internet_multiaddr_to_usual_format(
    multiaddr: &Multiaddr,
) -> Result<String, MultiaddrToStringConversionError> {
    use AddrComponent::*;
    use MultiaddrToStringConversionError::*;

    let mut components = multiaddr.iter();

    if let Some(first_component) = components.next() {
        let mut vec = Vec::new();
        match first_component {
            IP4(addr) => vec.extend(addr.to_string().bytes()),
            IP6(addr) => vec.extend(addr.to_string().bytes()),
            DNS4(name) => vec.extend(name.bytes()),
            DNS6(name) => vec.extend(name.bytes()),
            _ => return Err(IncorrectFirstComponent(first_component)),
        }

        if let Some(second_component) = components.next() {
            vec.push(b':');
            match second_component {
                TCP(port) => vec.extend(port.to_string().bytes()),
                UDP(port) => vec.extend(port.to_string().bytes()),
                // TODO: Perhaps just ignore it ?
                _ => return Err(IncorrectSecondComponent(second_component)),
            }
        }

        Ok(String::from_utf8_lossy(&vec[..]).to_string())
    } else {
        Err(MultiaddrToStringConversionError::EmptyMultiaddr)
    }
}

/// Error returned when [`try_internet_multiaddr_to_usual_format`] can't
/// successfully convert the given multiaddress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiaddrToStringConversionError {
    /// The multiaddress doesn't contain any components.
    EmptyMultiaddr,
    /// The first component of the multiaddress isn't supported, see
    /// [`try_internet_multiaddr_to_usual_format`] for a list of supported ones.
    IncorrectFirstComponent(AddrComponent),
    /// The second component, if provided, isn't supported, see
    /// [`try_internet_multiaddr_to_usual_format`] for a list of supported ones.
    IncorrectSecondComponent(AddrComponent),
}

impl fmt::Display for MultiaddrToStringConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for MultiaddrToStringConversionError {}

#[cfg(test)]
mod tests {
    use super::try_internet_multiaddr_to_usual_format;
    use super::MultiaddrToStringConversionError;
    use multiaddr::AddrComponent;

    #[test]
    fn it_can_parse_correct_addresses() {
        let addresses = vec![
            ("ip4", "127.0.0.1", "/tcp/", ":", "3333"),
            ("ip6", "::1", "/tcp/", ":", "3333"),
            ("ip4", "127.0.0.1", "/udp/", ":", "3333"),
            ("dns4", "rust-lange.org", "/tcp/", ":", "3333"),
            ("dns6", "rust-lange.org", "/tcp/", ":", "3333"),
            ("ip4", "127.0.0.1", "", "", ""),
        ];

        addresses
            .iter()
            .for_each(|(proto, addr, port_proto, colon, port)| {
                let usual_addr = format!("{}{}{}", addr, colon, port);
                let multiaddr = format!("/{}/{}{}{}", proto, addr, port_proto, port)
                    .parse()
                    .unwrap();

                let res = try_internet_multiaddr_to_usual_format(&multiaddr);
                assert_eq!(res, Ok(usual_addr));
            });
    }

    #[test]
    fn it_cant_parse_incorrect_addresses() {
        use AddrComponent::*;
        use MultiaddrToStringConversionError::*;

        let addresses = vec![
            ("", EmptyMultiaddr),
            (
                "/unix/socketname",
                IncorrectFirstComponent(UNIX("socketname".to_string())),
            ),
            (
                "/ip4/127.0.0.1/sctp/3333",
                IncorrectSecondComponent(SCTP(3333)),
            ),
        ];

        addresses.iter().for_each(|(multiaddr, error)| {
            let multiaddr = multiaddr.parse().expect(multiaddr);

            let res = try_internet_multiaddr_to_usual_format(&multiaddr);
            assert_eq!(res, Err(error.clone()));
        });
    }
}
