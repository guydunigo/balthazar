pub use wasmi::ModuleRef;
use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, Module,
    ModuleImportResolver, ModuleInstance, RuntimeArgs, RuntimeValue, Signature, Trap,
};

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::string::FromUtf8Error;

use super::task::arguments::argument_kind::ArgumentKind;
use super::task::arguments::Arguments;
use super::*;

const MAX_SOCKETS: i8 = 2;
const MAX_LISTENERS: i8 = 2;

//TODO: clean this mess!!!
//TODO: unwrap to proper errors
//TODO: prevent adding too many items to vec

#[derive(Debug)]
pub enum Error {
    InterpreterError(InterpreterError),
    ResultToStringError(FromUtf8Error),
}

impl From<InterpreterError> for Error {
    fn from(err: InterpreterError) -> Error {
        Error::InterpreterError(err)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Error {
        Error::ResultToStringError(err)
    }
}

struct Runtime<'a> {
    input: &'a [u8],
    input_counter: usize,
    output: Vec<u8>,
    socks: Vec<Option<TcpStream>>,
    listeners: Vec<Option<TcpListener>>,
}

impl<'a> Runtime<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        let mut r = Runtime {
            input,
            input_counter: 0,
            output: Vec::new(),
            socks: Vec::with_capacity(MAX_SOCKETS as usize),
            listeners: Vec::with_capacity(MAX_LISTENERS as usize),
        };

        for _ in 0..MAX_SOCKETS {
            r.socks.push(None);
        }
        for _ in 0..MAX_LISTENERS {
            r.listeners.push(None);
        }

        r
    }

    pub fn find_sock_id(&self) -> i8 {
        if let Some((id, _)) = self
            .socks
            .iter()
            .enumerate()
            .skip_while(|(_, s)| s.is_some())
            .next()
        {
            id as i8
        } else {
            -1
        }
    }
    pub fn find_listener_id(&self) -> i8 {
        if let Some((id, _)) = self
            .listeners
            .iter()
            .enumerate()
            .skip_while(|(_, s)| s.is_some())
            .next()
        {
            id as i8
        } else {
            -1
        }
    }
}

impl<'a> Externals for Runtime<'a> {
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
        println!("Executing function #{}", index);
        match index {
            0 => {
                let mut args_vec: Vec<u8> = Vec::with_capacity(32);
                for i in 0..args.len() {
                    if let Ok(n) = args.nth_checked(i) {
                        args_vec.push(n);
                    } /*else {
                        //TODO: return Err(Trap::new(TrapKind::Host())
                    }*/
                }

                print!("hash: 0x");
                args_vec.iter().for_each(|byte| print!("{:x}", byte));
                println!();

                Ok(None)
            }
            1 => {
                let byte: u8 = args.nth_checked(0).unwrap();
                self.output.push(byte);

                Ok(None)
            }
            2 => {
                let byte = if self.input_counter < self.input.len() {
                    let byte = self.input[self.input_counter];

                    self.input_counter += 1;
                    byte
                } else {
                    0
                };

                Ok(Some(RuntimeValue::from(byte)))
            }
            3 => Ok(Some(RuntimeValue::from(self.input.len() as u64))),
            // TODO: Well... This IS upgly... :
            // TODO: Some actual error codes
            4 => {
                let port: u16 = args.nth(0);
                let listener = TcpListener::bind(format!("localhost:{}", port));

                if let Err(_) = listener {
                    Ok(Some(RuntimeValue::from(-1)))
                } else if self.listeners.len() >= MAX_LISTENERS as usize {
                    Ok(Some(RuntimeValue::from(-2)))
                } else if let Ok(listener) = listener {
                    let id = self.find_listener_id();
                    self.listeners[id as usize] = Some(listener);

                    Ok(Some(RuntimeValue::from(id)))
                } else {
                    Ok(Some(RuntimeValue::from(-3)))
                }
            }
            5 => {
                let listener_id: i8 = args.nth(0);
                let sock_id = self.find_sock_id();

                let sock_id: i8 = if listener_id > MAX_LISTENERS || sock_id < 0 {
                    -1
                } else {
                    let listener = &self.listeners[listener_id as usize];

                    if let Some(listener) = listener {
                        let opt = listener.accept();

                        if let Ok((socket, _)) = opt {
                            self.socks[sock_id as usize] = Some(socket);

                            sock_id
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(sock_id)))
            }
            6 => {
                let sock_id: i8 = args.nth(0);

                let res = if sock_id > MAX_SOCKETS {
                    -1
                } else {
                    let sock_opt = &self.socks[sock_id as usize];
                    if let Some(sock) = sock_opt {
                        if let Ok(addr) = sock.peer_addr() {
                            if addr.is_ipv6() {
                                1
                            } else {
                                0
                            }
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(res)))
            }
            7 => {
                let sock_id: i8 = args.nth(0);
                // Check nth
                let nth: u8 = args.nth(1);

                let res: i16 = if sock_id > MAX_SOCKETS {
                    -1
                } else {
                    let sock_opt = &self.socks[sock_id as usize];
                    if let Some(sock) = sock_opt {
                        if let Ok(addr) = sock.peer_addr() {
                            match addr {
                                SocketAddr::V4(addr) => addr.ip().octets()[nth as usize] as i16,
                                SocketAddr::V6(addr) => addr.ip().octets()[nth as usize] as i16,
                            }
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(res)))
            }
            8 => {
                let sock_id: i8 = args.nth(0);

                let res: i16 = if sock_id > MAX_SOCKETS {
                    -1
                } else {
                    let sock_opt = &mut self.socks[sock_id as usize];
                    if let Some(sock) = sock_opt {
                        let mut buf: [u8; 1] = [0];
                        if let Ok(b) = sock.read(&mut buf[..]) {
                            if b == 0 {
                                (*sock_opt).take();
                                -1
                            } else {
                                buf[0] as i16
                            }
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(res)))
            }
            9 => {
                let sock_id: i8 = args.nth(0);
                let byte: u8 = args.nth(1);

                let res: i16 = if sock_id > MAX_SOCKETS {
                    -1
                } else {
                    let sock_opt = &mut self.socks[sock_id as usize];
                    if let Some(sock) = sock_opt {
                        let buf: [u8; 1] = [byte];
                        if let Ok(b) = sock.write(&buf[..]) {
                            if b == 0 {
                                (*sock_opt).take();
                                -1
                            } else {
                                buf[0] as i16
                            }
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(res)))
            }
            10 => {
                let sock_id: i8 = args.nth(0);

                let res: i16 = if sock_id > MAX_SOCKETS {
                    -1
                } else {
                    let sock_opt = self.socks[sock_id as usize].take();
                    if let Some(sock) = sock_opt {
                        if sock.shutdown(Shutdown::Both).is_ok() {
                            0
                        } else {
                            -1
                        }
                    } else {
                        -1
                    }
                };

                Ok(Some(RuntimeValue::from(res)))
            }
            _ => Err(Trap::new(wasmi::TrapKind::UnexpectedSignature)),
        }
    }
}

struct RuntimeModuleImportResolver;

impl<'a> ModuleImportResolver for RuntimeModuleImportResolver {
    fn resolve_func(
        &self,
        field_name: &str,
        signature: &Signature,
    ) -> Result<FuncRef, InterpreterError> {
        // println!("{} {:?}", field_name, signature);
        match field_name {
            // TODO: manually do the signature?
            // TODO: dynamically fetch the index?
            "return_256bits" => Ok(FuncInstance::alloc_host(signature.clone(), 0)),
            "push_byte" => Ok(FuncInstance::alloc_host(signature.clone(), 1)),
            "get_byte" => Ok(FuncInstance::alloc_host(signature.clone(), 2)),
            "get_bytes_len" => Ok(FuncInstance::alloc_host(signature.clone(), 3)),
            "tcp_listen_init" => Ok(FuncInstance::alloc_host(signature.clone(), 4)),
            "tcp_accept" => Ok(FuncInstance::alloc_host(signature.clone(), 5)),
            "tcp_socket_addr_is_v6" => Ok(FuncInstance::alloc_host(signature.clone(), 6)),
            "tcp_socket_addr_nth" => Ok(FuncInstance::alloc_host(signature.clone(), 7)),
            "tcp_read_byte" => Ok(FuncInstance::alloc_host(signature.clone(), 8)),
            "tcp_write_byte" => Ok(FuncInstance::alloc_host(signature.clone(), 9)),
            "tcp_close" => Ok(FuncInstance::alloc_host(signature.clone(), 10)),
            _ => Err(InterpreterError::Function(format!(
                "host module doesn't export function with name {}",
                field_name
            ))),
        }
    }
}

pub fn _get_functions_list(bytecode: &[u8]) {
    let module: parity_wasm::elements::Module = parity_wasm::deserialize_buffer(bytecode).unwrap();
    let module = module.parse_names().unwrap();
    let names_section = module.names_section().unwrap();
    if let parity_wasm::elements::NameSection::Function(names_section) = names_section {
        names_section
            .names()
            .iter()
            //.take(10)
            .for_each(|t| println!("{}", t.1));
        //println!("J: {}", names_section.names());
    }
}

pub fn exec_wasm(
    bytecode: &[u8],
    args: &Arguments,
    instance: Option<&ModuleRef>,
) -> Result<(Arguments, Option<ModuleRef>), Error> {
    let mut runtime = Runtime::new(&args.bytes[..]);

    let args: Vec<RuntimeValue> = args.args.iter().map(|a| a.to_runtime_value()).collect();

    // This little gymnastic is done to be able to cache job_instances.
    // TODO: There is probably a much cleaner way... :/
    let res = match instance {
        None => {
            let module = Module::from_buffer(bytecode)?;
            let resolver = RuntimeModuleImportResolver;
            let imports = ImportsBuilder::new().with_resolver("env", &resolver);
            let instance = ModuleInstance::new(&module, &imports)?.assert_no_start();

            (
                instance.invoke_export("start", &args[..], &mut runtime)?,
                Some(instance),
            )
        }
        Some(instance) => (
            instance.invoke_export("start", &args[..], &mut runtime)?,
            None,
        ),
    };

    let return_value = if let (Some(res), _) = res {
        match res {
            RuntimeValue::I32(res) => println!("Return value: {}", res),
            RuntimeValue::I64(res) => println!("Return value: {}", res),
            res => println!("Return value unknown: {:?}", res),
        }
        vec![ArgumentKind::from(res)]
    } else {
        println!("No return value.");
        Vec::new()
    };

    println!(
        "Returned text: '{}'",
        String::from_utf8_lossy(runtime.output.as_slice())
    );

    let ret_args = Arguments::new(return_value.as_slice(), runtime.output.as_slice());
    let instance = res.1;
    Ok((ret_args, instance))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_execs_wasm() {
        unimplemented!();
    }
}
