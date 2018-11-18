pub mod argument_kind;
use self::argument_kind::ArgumentKind;

use std::default::Default;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash)]
// TODO: Options rather than Vec ?
pub struct Arguments {
    pub args: Vec<ArgumentKind>,
    pub bytes: Vec<u8>,
}

impl Arguments {
    pub fn new(args: &[ArgumentKind], bytes: &[u8]) -> Arguments {
        Arguments {
            args: Vec::from(args),
            bytes: Vec::from(bytes),
        }
    }
}

impl Default for Arguments {
    fn default() -> Arguments {
        Arguments {
            args: Vec::new(),
            bytes: Vec::new(),
        }
    }
}
