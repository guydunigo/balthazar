use std::time::Instant;

pub fn run(args: Vec<u8>, nb_times: usize) -> Result<(), i64> {
    let inst_read = Instant::now();
    let nb_times = if nb_times == 0 { 1 } else { nb_times };

    let result = wasm::my_run(args.clone())?;
    for _ in 0..(nb_times - 1) {
        wasm::my_run(args.clone())?;
    }
    let inst_res = Instant::now();

    println!(
        "{:?} gives {:?}",
        String::from_utf8_lossy(&args[..]),
        String::from_utf8_lossy(&result[..])
    );
    println!(
        "times:\n- running all {}ms\n- running average {}ms",
        (inst_res - inst_read).as_millis(),
        (inst_res - inst_read).as_millis() / (nb_times as u128),
    );

    Ok(())
}
