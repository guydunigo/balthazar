use wasmi::{ImportsBuilder, Module, ModuleInstance, NopExternals, RuntimeArgs, RuntimeValue};

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

    let instance = ModuleInstance::new(&module, &ImportsBuilder::default())
        .expect("failed to instantiate wasm module")
        .assert_no_start();

    let res = instance
            .invoke_export("start", &[], &mut NopExternals)
            .expect("failed to execute export");

    if let Some(RuntimeValue::I32(res)) = res {
        println!(
            "start : {:x}", res
        );
    }

    Ok(0)
}
