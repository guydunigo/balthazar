use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, Module,
    ModuleImportResolver, ModuleInstance, RuntimeArgs, RuntimeValue, Signature, Trap,
};

//TODO: clean this mess!!!
//TODO: unwrap to proper errors

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
                    args_vec.push(args.nth_checked(i).unwrap());
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
        let func_ref = match field_name {
            // TODO: manually do the signature?
            // TODO: dynamically fetch the index?
            "return_32bytes" => FuncInstance::alloc_host(signature.clone(), 0),
            "push_char" => FuncInstance::alloc_host(signature.clone(), 1),
            _ => {
                return Err(InterpreterError::Function(format!(
                    "host module doesn't export function with name {}",
                    field_name
                )))
            }
        };
        Ok(func_ref)
    }
}

pub fn get_functions_list(bytecode: Vec<u8>) {
    let module: parity_wasm::elements::Module =
        parity_wasm::deserialize_buffer(&bytecode[..]).unwrap();
    let module = module.parse_names().unwrap();
    let names_section = module.names_section().unwrap();
    if let parity_wasm::elements::NameSection::Function(names_section) = names_section {
        names_section
            .names()
            .iter()
            .take(10)
            .for_each(|t| println!("{}", t.1));
        //println!("J: {}", names_section.names());
    }
}

pub fn exec_wasm(bytecode: Vec<u8>) -> Result<u8, u8> {
    let module = Module::from_buffer(&bytecode).expect("test");

    let resolver = RuntimeModuleImportResolver;
    let imports = ImportsBuilder::new().with_resolver("env", &resolver);
    let instance = ModuleInstance::new(&module, &imports)
        .expect("failed to instantiate wasm module")
        .assert_no_start();

    let mut runtime = Runtime { txt: Vec::new() };
    let res = instance
        .invoke_export("start", &[], &mut runtime)
        .expect("failed to execute export");

    if let Some(RuntimeValue::I32(res)) = res {
        println!("start : {:x}", res);
    } else {
        println!("start : {:?}", res);
    }

    println!(
        "hash src: {}",
        String::from_utf8_lossy(runtime.txt.as_slice())
    );

    Ok(0)
}
