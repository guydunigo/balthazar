extern crate balthaproto as proto;

mod node_type;
pub use node_type::NodeType;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
