fn main() {
    subwy::run(std::env::args_os().nth(1).map(std::path::PathBuf::from));
}
