// The colored output here is very primitive and only works for ANSI
// terminals. We'd like to use ansterm and owo_style like cargo does (see
// https://docs.rs/anstream/0.6.4/anstream/) but that requeires Rust 1.70.0
// and we are stuck on 1.68

#[macro_export]
macro_rules! status {
    ($prefix:expr, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        println!("\x1b[1;32m{:>30}\x1b[0m {}", $prefix, msg);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }
}

#[macro_export]
macro_rules! error {
    ($prefix:expr, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        println!("\x1b[1;31m{:>30}\x1b[0m {}", $prefix, msg);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }
}
