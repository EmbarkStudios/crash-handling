fn main() {
    let mut build = cc::Build::new();

    build.file("src/sadness.c");

    build.compile("sadness");
}
