use std::time::Instant;

pub fn run(args: Vec<Vec<u8>>, nb_times: usize) -> Result<(), i64> {
    let inst_read = Instant::now();
    let nb_times = if nb_times == 0 { 1 } else { nb_times };

    let mut results = Vec::with_capacity(args.len());

    for arg in args.iter() {
        let result = wasm::my_run(arg.clone())?;
        for _ in 0..(nb_times - 1) {
            wasm::my_run(arg.clone())?;
        }
        let inst_res = Instant::now();

        println!(
            "{} gives {}",
            String::from_utf8_lossy(&arg[..]),
            String::from_utf8_lossy(&result[..])
        );
        println!(
            "times:\n- running all {}ms\n- running average {}ms",
            (inst_res - inst_read).as_millis(),
            (inst_res - inst_read).as_millis() / (nb_times as u128),
        );
        results.push((arg, result));
    }

    let inst_res = Instant::now();
    println!("Total:");
    println!(
        "times:\n- running all {}ms\n- running average {}ms",
        (inst_res - inst_read).as_millis(),
        (inst_res - inst_read).as_millis() / ((args.len() * nb_times) as u128),
    );

    for (arg, result) in results.drain(..) {
        print!(
            "Testing {} : {}... ",
            String::from_utf8_lossy(&arg[..]),
            String::from_utf8_lossy(&result[..])
        );
        let test_result = wasm::test_result(&arg[..], result)?;
        if test_result.is_some() {
            println!("passed.");
        } else {
            println!("failed.");
        }
    }

    Ok(())
}
