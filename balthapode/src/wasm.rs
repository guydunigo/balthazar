use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, Module,
    ModuleImportResolver, ModuleInstance, RuntimeArgs, RuntimeValue, Signature, Trap,
};

use std::string::FromUtf8Error;

use job::task::arguments::argument_kind::ArgumentKind;
use job::task::arguments::Arguments;

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

struct Runtime {
    txt: Vec<u8>,
}

impl Externals for Runtime {
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
                println!("");
            }
            1 => {
                let byte: u8 = args.nth_checked(0).unwrap();
                self.txt.push(byte);
            }
            _ => return Err(Trap::new(wasmi::TrapKind::UnexpectedSignature)),
        }

        Ok(None)
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
            "push_char" => Ok(FuncInstance::alloc_host(signature.clone(), 1)),
            _ => Err(InterpreterError::Function(format!(
                "host module doesn't export function with name {}",
                field_name
            ))),
        }
    }
}

pub fn _get_functions_list(bytecode: Vec<u8>) {
    let module: parity_wasm::elements::Module =
        parity_wasm::deserialize_buffer(&bytecode[..]).unwrap();
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

// TODO: Use arguments
pub fn exec_wasm(bytecode: Vec<u8>, args: Arguments) -> Result<Arguments, Error> {
    let module = Module::from_buffer(&bytecode)?;

    let args: Vec<RuntimeValue> = args.args.iter().map(|a| a.to_runtime_value()).collect();

    let resolver = RuntimeModuleImportResolver;
    let imports = ImportsBuilder::new().with_resolver("env", &resolver);
    let instance = ModuleInstance::new(&module, &imports)?.assert_no_start();

    let mut runtime = Runtime { txt: Vec::new() };
    let res = instance.invoke_export("start", &args[..], &mut runtime)?;

    let return_value = if let Some(res) = res {
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
        String::from_utf8_lossy(runtime.txt.as_slice())
    );

    let ret_args = Arguments::new(return_value.as_slice(), runtime.txt.as_slice());
    Ok(ret_args)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_execs_wasm() {
        unimplemented!();
    }
}
