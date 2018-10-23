pub use wasmi::ModuleRef;
use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, Module,
    ModuleImportResolver, ModuleInstance, RuntimeArgs, RuntimeValue, Signature, Trap,
};

use std::string::FromUtf8Error;

use super::task::arguments::argument_kind::ArgumentKind;
use super::task::arguments::Arguments;
use super::*;

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
}

impl<'a> Runtime<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Runtime {
            input,
            input_counter: 0,
            output: Vec::new(),
        }
    }
}

impl<'a> Externals for Runtime<'a> {
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
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
