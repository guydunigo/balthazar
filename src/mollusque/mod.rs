mod cephalo;
mod pode;

pub use self::cephalo::Cephalo;
pub use self::pode::Pode;

use std::io;

pub trait Mollusque {
    // Can't run with tentacles...
    fn swim(&mut self) -> io::Result<()>;
}

pub enum CephalopodeType {
    Cephalo,
    Pode,
}
