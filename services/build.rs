fn main() {
    cc::Build::new().file("cpp/log.c").compile("c_log");
}
