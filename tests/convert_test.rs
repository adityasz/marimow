use generate_tests::generate_file_tests;

generate_file_tests!("tests/data";
    "empty.py",
    "only_comments.py",
    "no_init.py",
    "only_init.py",
    "one_cell.py",
);
