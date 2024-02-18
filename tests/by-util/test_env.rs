// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore (words) bamf chdir rlimit prlimit COMSPEC cout cerr

#[cfg(any(target_os = "linux", target_os = "android"))]
use crate::common::util::expected_result;
use crate::common::util::TestScenario;
use std::env;
use std::ffi::OsString;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(125);
}

#[test]
fn test_env_help() {
    new_ucmd!()
        .arg("--help")
        .succeeds()
        .no_stderr()
        .stdout_contains("Options:");
}

#[test]
fn test_env_version() {
    new_ucmd!()
        .arg("--version")
        .succeeds()
        .no_stderr()
        .stdout_contains(util_name!());
}

#[test]
fn test_echo() {
    #[cfg(target_os = "windows")]
    let args = ["cmd", "/d/c", "echo"];
    #[cfg(not(target_os = "windows"))]
    let args = ["echo"];

    let result = new_ucmd!().args(&args).arg("FOO-bar").succeeds();

    assert_eq!(result.stdout_str().trim(), "FOO-bar");
}

#[cfg(target_os = "windows")]
#[test]
fn test_if_windows_batch_files_can_be_executed() {
    let result = new_ucmd!().arg("./runBat.bat").succeeds();

    assert!(result.stdout_str().contains("Hello Windows World!"));
}

#[test]
fn test_file_option() {
    let out = new_ucmd!()
        .arg("-f")
        .arg("vars.conf.txt")
        .run()
        .stdout_move_str();

    assert_eq!(
        out.lines()
            .filter(|&line| line == "FOO=bar" || line == "BAR=bamf this")
            .count(),
        2
    );
}

#[test]
fn test_combined_file_set() {
    let out = new_ucmd!()
        .arg("-f")
        .arg("vars.conf.txt")
        .arg("FOO=bar.alt")
        .run()
        .stdout_move_str();

    assert_eq!(out.lines().filter(|&line| line == "FOO=bar.alt").count(), 1);
}

#[test]
fn test_combined_file_set_unset() {
    let out = new_ucmd!()
        .arg("-u")
        .arg("BAR")
        .arg("-f")
        .arg("vars.conf.txt")
        .arg("FOO=bar.alt")
        .succeeds()
        .stdout_move_str();

    assert_eq!(
        out.lines()
            .filter(|&line| line == "FOO=bar.alt" || line.starts_with("BAR="))
            .count(),
        1
    );
}

#[test]
fn test_unset_invalid_variables() {
    use uucore::display::Quotable;

    // Cannot test input with \0 in it, since output will also contain \0. rlimit::prlimit fails
    // with this error: Error { kind: InvalidInput, message: "nul byte found in provided data" }
    for var in ["", "a=b"] {
        new_ucmd!().arg("-u").arg(var).run().stderr_only(format!(
            "env: cannot unset {}: Invalid argument\n",
            var.quote()
        ));
    }
}

#[test]
fn test_single_name_value_pair() {
    let out = new_ucmd!().arg("FOO=bar").run();

    assert!(out.stdout_str().lines().any(|line| line == "FOO=bar"));
}

#[test]
fn test_multiple_name_value_pairs() {
    let out = new_ucmd!().arg("FOO=bar").arg("ABC=xyz").run();

    assert_eq!(
        out.stdout_str()
            .lines()
            .filter(|&line| line == "FOO=bar" || line == "ABC=xyz")
            .count(),
        2
    );
}

#[test]
fn test_ignore_environment() {
    let scene = TestScenario::new(util_name!());

    scene.ucmd().arg("-i").succeeds().no_stdout();
    scene.ucmd().arg("-").succeeds().no_stdout();
}

#[test]
fn test_empty_name() {
    new_ucmd!()
        .arg("-i")
        .arg("=xyz")
        .run()
        .stderr_only("env: warning: no name specified for value 'xyz'\n");
}

#[test]
fn test_null_delimiter() {
    let out = new_ucmd!()
        .arg("-i")
        .arg("--null")
        .arg("FOO=bar")
        .arg("ABC=xyz")
        .succeeds()
        .stdout_move_str();

    let mut vars: Vec<_> = out.split('\0').collect();
    assert_eq!(vars.len(), 3);
    vars.sort_unstable();
    assert_eq!(vars[0], "");
    assert_eq!(vars[1], "ABC=xyz");
    assert_eq!(vars[2], "FOO=bar");
}

#[test]
fn test_unset_variable() {
    let out = TestScenario::new(util_name!())
        .ucmd()
        .env("HOME", "FOO")
        .arg("-u")
        .arg("HOME")
        .succeeds()
        .stdout_move_str();

    assert!(!out.lines().any(|line| line.starts_with("HOME=")));
}

#[test]
fn test_fail_null_with_program() {
    new_ucmd!()
        .arg("--null")
        .arg("cd")
        .fails()
        .stderr_contains("cannot specify --null (-0) with command");
}

#[cfg(not(windows))]
#[test]
fn test_change_directory() {
    let scene = TestScenario::new(util_name!());
    let temporary_directory = tempdir().unwrap();
    let temporary_path = std::fs::canonicalize(temporary_directory.path()).unwrap();
    assert_ne!(env::current_dir().unwrap(), temporary_path);

    // command to print out current working directory
    let pwd = "pwd";

    let out = scene
        .ucmd()
        .arg("--chdir")
        .arg(&temporary_path)
        .arg(pwd)
        .succeeds()
        .stdout_move_str();
    assert_eq!(out.trim(), temporary_path.as_os_str());
}

#[cfg(windows)]
#[test]
fn test_change_directory() {
    let scene = TestScenario::new(util_name!());
    let temporary_directory = tempdir().unwrap();

    let temporary_path = temporary_directory.path();
    let temporary_path = temporary_path
        .strip_prefix(r"\\?\")
        .unwrap_or(temporary_path);

    let env_cd = env::current_dir().unwrap();
    let env_cd = env_cd.strip_prefix(r"\\?\").unwrap_or(&env_cd);

    assert_ne!(env_cd, temporary_path);

    // COMSPEC is a variable that contains the full path to cmd.exe
    let cmd_path = env::var("COMSPEC").unwrap();

    // command to print out current working directory
    let pwd = [&*cmd_path, "/C", "cd"];

    let out = scene
        .ucmd()
        .arg("--chdir")
        .arg(temporary_path)
        .args(&pwd)
        .succeeds()
        .stdout_move_str();
    assert_eq!(out.trim(), temporary_path.as_os_str());
}

#[test]
fn test_fail_change_directory() {
    let scene = TestScenario::new(util_name!());
    let some_non_existing_path = "some_nonexistent_path";
    assert!(!Path::new(some_non_existing_path).is_dir());

    let out = scene
        .ucmd()
        .arg("--chdir")
        .arg(some_non_existing_path)
        .arg("pwd")
        .fails()
        .stderr_move_str();
    assert!(out.contains("env: cannot change directory to "));
}

fn modify_newlines_according_platform(input: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        input.replace("\n", "\r\n")
    }
    #[cfg(not(target_os = "windows"))]
    {
        input.into()
    }
}

#[cfg(not(target_os = "windows"))] // windows has no executable "echo", its only supported as part of a batch-file
#[test]
fn test_split_string_into_args_one_argument_no_quotes() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .arg("-S echo hello world")
        .succeeds()
        .stdout_move_str();
    assert_eq!(out, modify_newlines_according_platform("hello world\n"));
}

#[cfg(not(target_os = "windows"))] // windows has no executable "echo", its only supported as part of a batch-file
#[test]
fn test_split_string_into_args_one_argument() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .arg("-S echo \"hello world\"")
        .succeeds()
        .stdout_move_str();
    assert_eq!(out, modify_newlines_according_platform("hello world\n"));
}

#[cfg(not(target_os = "windows"))] // windows has no executable "echo", its only supported as part of a batch-file
#[test]
fn test_split_string_into_args_s_escaping_challenge() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .args(&[r#"-S echo "hello \"great\" world""#])
        .succeeds()
        .stdout_move_str();
    assert_eq!(
        out,
        modify_newlines_according_platform("hello \"great\" world\n")
    );
}

#[test]
fn test_split_string_into_args_s_escaped_c_not_allowed() {
    let scene = TestScenario::new(util_name!());

    let out = scene.ucmd().args(&[r#"-S"\c""#]).fails().stderr_move_str();
    assert_eq!(
        out,
        "env: '\\c' must not appear in double-quoted -S string\n"
    );
}

#[cfg(not(target_os = "windows"))] // no printf available
#[test]
fn test_split_string_into_args_s_whitespace_handling() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .args(&["-Sprintf x%sx\\n A \t B \x0B\x0C\r\n"])
        .succeeds()
        .stdout_move_str();
    assert_eq!(out, "xAx\nxBx\n");
}

#[cfg(not(target_os = "windows"))] // no printf available
#[test]
fn test_split_string_into_args_long_option_whitespace_handling() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .args(&["--split-string printf x%sx\\n A \t B \x0B\x0C\r\n"])
        .succeeds()
        .stdout_move_str();
    assert_eq!(out, "xAx\nxBx\n");
}

#[cfg(not(target_os = "windows"))] // no printf available
#[test]
fn test_split_string_into_args_debug_output_whitespace_handling() {
    let scene = TestScenario::new(util_name!());

    let out = scene
        .ucmd()
        .args(&["-vS printf x%sx\\n A \t B \x0B\x0C\r\n"])
        .succeeds();
    assert_eq!(out.stdout_str(), "xAx\nxBx\n");
    assert_eq!(out.stderr_str(), "input args:\narg[0]: 'env'\narg[1]: $'-vS printf x%sx\\\\n A \\t B \\x0B\\x0C\\r\\n'\nexecutable: 'printf'\narg[0]: $'x%sx\\n'\narg[1]: 'A'\narg[2]: 'B'\n");
}

#[test]
fn test_gnu_e20() {
    let scene = TestScenario::new(util_name!());

    let env_bin = String::from(crate::common::util::TESTS_BINARY) + " " + util_name!();

    let (input, output) = (
        [
            String::from("-i"),
            String::from(r#"-SA="B\_C=D" "#) + env_bin.as_str() + "",
        ],
        "A=B C=D\n",
    );

    let out = scene.ucmd().args(&input).succeeds();
    assert_eq!(out.stdout_str(), output);
}

#[test]
fn test_split_string_misc() {
    use ::env::parse_args_from_str;

    assert_eq!(
        vec!["A=B", "FOO=AR", "sh", "-c", "echo $A$FOO"],
        parse_args_from_str(&OsString::from(r#"A=B FOO=AR  sh -c "echo \$A\$FOO""#)).unwrap(),
    );
    assert_eq!(
        vec!["A=B", "FOO=AR", "sh", "-c", "echo $A$FOO"],
        parse_args_from_str(&OsString::from(r#"A=B FOO=AR  sh -c 'echo $A$FOO'"#)).unwrap(),
    );
    assert_eq!(
        vec!["A=B", "FOO=AR", "sh", "-c", "echo $A$FOO"],
        parse_args_from_str(&OsString::from(r#"A=B FOO=AR  sh -c 'echo $A$FOO'"#)).unwrap(),
    );

    assert_eq!(
        vec!["-i", "A=B ' C"],
        parse_args_from_str(&OsString::from(r#"-i A='B \' C'"#)).unwrap(),
    );
}

#[test]
fn test_split_string_environment_vars_test() {
    std::env::set_var("FOO", "BAR");
    assert_eq!(
        vec!["FOO=bar", "sh", "-c", "echo xBARx =$FOO="],
        ::env::parse_args_from_str(&OsString::from(r#"FOO=bar sh -c "echo x${FOO}x =\$FOO=""#))
            .unwrap(),
    );
}

#[macro_export]
macro_rules! compare_with_gnu {
    ( $ts:expr, $args:expr ) => {{
        eprintln!("==========================================================================");
        let result = $ts.ucmd().args($args).run();

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let reference = expected_result(&$ts, $args);
            if let Ok(reference) = reference {
                let success = result.code() == reference.code()
                    && result.stdout_str() == reference.stdout_str()
                    && result.stderr_str() == reference.stderr_str();
                if !success {
                    eprintln!("reference.code: {}", reference.code());
                    eprintln!("   result.code: {}", result.code());
                    eprintln!("reference.cout: {}", reference.stdout_str());
                    eprintln!("   result.cout: {}", result.stdout_str());
                    eprintln!("reference.cerr: {}", reference.stderr_str_lossy());
                    eprintln!("   result.cerr: {}", result.stderr_str_lossy());
                }
                assert_eq!(result.code(), reference.code());
                assert_eq!(result.stdout_str(), reference.stdout_str());
                assert_eq!(result.stderr_str(), reference.stderr_str());
            } else {
                println!(
                    "gnu reference test skipped. Reason: {:?}",
                    reference.unwrap_err()
                );
            }
        }

        result
    }};
}

#[test]
fn test_env_with_gnu_reference() {
    let ts = TestScenario::new(util_name!());

    compare_with_gnu!(ts, &["-S\\|echo hallo"]) // no quotes, invalid escape sequence |
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\|' in -S\n");

    compare_with_gnu!(ts, &["-S\\a"]) // no quotes, invalid escape sequence a
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\a' in -S\n");

    compare_with_gnu!(ts, &["-S\"\\a\""]) // double quotes, invalid escape sequence a
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\a' in -S\n");

    compare_with_gnu!(ts, &[r#"-S"\a""#]) // same as before, just using r#""#
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\a' in -S\n");

    compare_with_gnu!(ts, &["-S'\\a'"]) // single quotes, invalid escape sequence a
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\a' in -S\n");

    compare_with_gnu!(ts, &[r#"-S\|\&\;"#]) // no quotes, invalid escape sequence |
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\|' in -S\n");

    compare_with_gnu!(ts, &[r#"-S\<\&\;"#]) // no quotes, invalid escape sequence <
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\<' in -S\n");

    compare_with_gnu!(ts, &[r#"-S\>\&\;"#]) // no quotes, invalid escape sequence >
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\>' in -S\n");

    compare_with_gnu!(ts, &[r#"-S\`\&\;"#]) // no quotes, invalid escape sequence `
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    compare_with_gnu!(ts, &[r#"-S"\`\&\;""#]) // double quotes, invalid escape sequence `
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    compare_with_gnu!(ts, &[r#"-S'\`\&\;'"#]) // single quotes, invalid escape sequence `
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    ts.ucmd()
        .args(&["-S\\'\\'"]) // empty single quotes, considered as program name
        .fails()
        .code_is(127)
        .no_stdout()
        .stderr_is("env: \"''\": No such file or directory\n"); // gnu version again adds escaping here

    compare_with_gnu!(ts, &["-S\\\"\\\""]) // empty double quotes, considered as program name
        .failure()
        .code_is(127)
        .no_stdout()
        .stderr_is("env: '\"\"': No such file or directory\n");

    compare_with_gnu!(ts, &[r#"-S\`"#]) // ` escaped without quotes
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    compare_with_gnu!(ts, &[r#"-S"\`""#]) // ` escaped in double quotes
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    compare_with_gnu!(ts, &[r#"-S'\`'"#]) // ` escaped in single quotes
        .failure()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\`' in -S\n");

    ts.ucmd()
        .args(&[r#"-S\🦉"#]) // ` escaped in single quotes
        .fails()
        .code_is(125)
        .no_stdout()
        .stderr_is("env: invalid sequence '\\🦉' in -S\n"); // gnu doesn't show the owl. Instead a invalid unicode ?
}

#[cfg(test)]
mod tests_split_iterator {

    enum EscapeStyle {
        /// No escaping.
        None,
        /// Wrap in single quotes.
        SingleQuoted,
        /// Single quotes combined with backslash.
        Mixed,
    }

    /// Determines escaping style to use.
    fn escape_style(s: &str) -> EscapeStyle {
        if s.is_empty() {
            return EscapeStyle::SingleQuoted;
        }

        let mut special = false;
        let mut newline = false;
        let mut single_quote = false;

        for c in s.chars() {
            match c {
                '\n' => {
                    newline = true;
                    special = true;
                }
                '\'' => {
                    single_quote = true;
                    special = true;
                }
                '|' | '&' | ';' | '<' | '>' | '(' | ')' | '$' | '`' | '\\' | '"' | ' ' | '\t'
                | '*' | '?' | '[' | '#' | '˜' | '=' | '%' => {
                    special = true;
                }
                _ => continue,
            }
        }

        if !special {
            EscapeStyle::None
        } else if newline && !single_quote {
            EscapeStyle::SingleQuoted
        } else {
            EscapeStyle::Mixed
        }
    }

    /// Escapes special characters in a string, so that it will retain its literal
    /// meaning when used as a part of command in Unix shell.
    ///
    /// It tries to avoid introducing any unnecessary quotes or escape characters,
    /// but specifics regarding quoting style are left unspecified.
    pub fn quote(s: &str) -> std::borrow::Cow<str> {
        // We are going somewhat out of the way to provide
        // minimal amount of quoting in typical cases.
        match escape_style(s) {
            EscapeStyle::None => s.into(),
            EscapeStyle::SingleQuoted => format!("'{}'", s).into(),
            EscapeStyle::Mixed => {
                let mut quoted = String::new();
                quoted.push('\'');
                for c in s.chars() {
                    if c == '\'' {
                        quoted.push_str("'\\''");
                    } else {
                        quoted.push(c);
                    }
                }
                quoted.push('\'');
                quoted.into()
            }
        }
    }

    /// Joins arguments into a single command line suitable for execution in Unix
    /// shell.
    ///
    /// Each argument is quoted using [`quote`] to preserve its literal meaning when
    /// parsed by Unix shell.
    ///
    /// Note: This function is essentially an inverse of [`split`].
    ///
    /// # Examples
    ///
    /// Logging executed commands in format that can be easily copied and pasted
    /// into an actual shell:
    ///
    /// ```rust,no_run
    /// fn execute(args: &[&str]) {
    ///     use std::process::Command;
    ///     println!("Executing: {}", shell_words::join(args));
    ///     Command::new(&args[0])
    ///         .args(&args[1..])
    ///         .spawn()
    ///         .expect("failed to start subprocess")
    ///         .wait()
    ///         .expect("failed to wait for subprocess");
    /// }
    ///
    /// execute(&["python", "-c", "print('Hello world!')"]);
    /// ```
    ///
    /// [`quote`]: fn.quote.html
    /// [`split`]: fn.split.html
    pub fn join<I, S>(words: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut line = words.into_iter().fold(String::new(), |mut line, word| {
            let quoted = quote(word.as_ref());
            line.push_str(quoted.as_ref());
            line.push(' ');
            line
        });
        line.pop();
        line
    }

    use std::ffi::OsString;

    use ::env::parse_error::ParseError;

    fn split(input: &str) -> Result<Vec<OsString>, ParseError> {
        ::env::split_iterator::split(&OsString::from(input))
    }

    fn split_ok(cases: &[(&str, &[&str])]) {
        for (i, &(input, expected)) in cases.iter().enumerate() {
            match split(input) {
                Err(actual) => {
                    panic!(
                        "[{i}] calling split({:?}):\nexpected: Ok({:?})\n  actual: Err({:?})\n",
                        input, expected, actual
                    );
                }
                Ok(actual) => {
                    assert!(
                        expected == actual.as_slice(),
                        "[{i}] After split({:?}).unwrap()\nexpected: {:?}\n  actual: {:?}\n",
                        input,
                        expected,
                        actual
                    );
                }
            }
        }
    }

    #[test]
    fn split_empty() {
        split_ok(&[("", &[])]);
    }

    #[test]
    fn split_initial_whitespace_is_removed() {
        split_ok(&[
            ("     a", &["a"]),
            ("\t\t\t\tbar", &["bar"]),
            ("\t \nc", &["c"]),
        ]);
    }

    #[test]
    fn split_trailing_whitespace_is_removed() {
        split_ok(&[
            ("a  ", &["a"]),
            ("b\t", &["b"]),
            ("c\t \n \n \n", &["c"]),
            ("d\n\n", &["d"]),
        ]);
    }

    #[test]
    fn split_carriage_return() {
        split_ok(&[("c\ra\r'\r'\r", &["c", "a", "\r"])]);
    }

    #[test]
    fn split_() {
        split_ok(&[("\\'\\'", &["''"])]);
    }

    #[test]
    fn split_single_quotes() {
        split_ok(&[
            (r#"''"#, &[r#""#]),
            (r#"'a'"#, &[r#"a"#]),
            (r#"'\\'"#, &[r#"\"#]),
            (r#"' \\ '"#, &[r#" \ "#]),
            (r#"'#'"#, &[r#"#"#]),
        ]);
    }

    #[test]
    fn split_double_quotes() {
        split_ok(&[
            (r#""""#, &[""]),
            (r#""""""#, &[""]),
            (r#""a b c' d""#, &["a b c' d"]),
            (r#""\$""#, &["$"]),
            (r#""`""#, &["`"]),
            (r#""\"""#, &["\""]),
            (r#""\\""#, &["\\"]),
            ("\"\n\"", &["\n"]),
            ("\"\\\n\"", &[""]),
        ]);
    }

    #[test]
    fn split_unquoted() {
        split_ok(&[
            (r#"\\|\\&\\;"#, &[r#"\|\&\;"#]),
            (r#"\\<\\>"#, &[r#"\<\>"#]),
            (r#"\\(\\)"#, &[r#"\(\)"#]),
            (r#"\$"#, &[r#"$"#]),
            (r#"\""#, &[r#"""#]),
            (r#"\'"#, &[r#"'"#]),
            ("\\\n", &[]),
            (" \\\n \n", &[]),
            ("a\nb\nc", &["a", "b", "c"]),
            ("a\\\nb\\\nc", &["abc"]),
            ("foo bar baz", &["foo", "bar", "baz"]),
        ]);
    }

    #[test]
    fn split_trailing_backslash() {
        assert_eq!(
            split("\\"),
            Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                pos: 1,
                quoting: "Delimiter".into()
            })
        );
        assert_eq!(
            split(" \\"),
            Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                pos: 2,
                quoting: "Delimiter".into()
            })
        );
        assert_eq!(
            split("a\\"),
            Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                pos: 2,
                quoting: "Unquoted".into()
            })
        );
    }

    #[test]
    fn split_errors() {
        assert_eq!(
            split("'abc"),
            Err(ParseError::MissingClosingQuote { pos: 4, c: '\'' })
        );
        assert_eq!(
            split("\""),
            Err(ParseError::MissingClosingQuote { pos: 1, c: '"' })
        );
        assert_eq!(
            split("'\\"),
            Err(ParseError::MissingClosingQuote { pos: 2, c: '\'' })
        );
        assert_eq!(
            split("'\\"),
            Err(ParseError::MissingClosingQuote { pos: 2, c: '\'' })
        );
        assert_eq!(
            split(r#""$""#),
            Err(ParseError::ParsingOfVariableNameFailed {
                pos: 2,
                msg: "Missing variable name".into()
            }),
        );
    }

    #[test]
    fn split_error_fail_with_unknown_escape_sequences() {
        assert_eq!(
            split("\\a"),
            Err(ParseError::InvalidSequenceBackslashXInMinusS { pos: 1, c: 'a' })
        );
        assert_eq!(
            split("\"\\a\""),
            Err(ParseError::InvalidSequenceBackslashXInMinusS { pos: 2, c: 'a' })
        );
        assert_eq!(
            split("'\\a'"),
            Err(ParseError::InvalidSequenceBackslashXInMinusS { pos: 2, c: 'a' })
        );
        assert_eq!(
            split(r#""\a""#),
            Err(ParseError::InvalidSequenceBackslashXInMinusS { pos: 2, c: 'a' })
        );
        assert_eq!(
            split(r#"\🦉"#),
            Err(ParseError::InvalidSequenceBackslashXInMinusS { pos: 1, c: '🦉' })
        );
    }

    #[test]
    fn split_comments() {
        split_ok(&[
            (r#" x # comment "#, &["x"]),
            (r#" w1#w2 "#, &["w1#w2"]),
            (r#"'not really a # comment'"#, &["not really a # comment"]),
            (" a # very long comment \n b # another comment", &["a", "b"]),
        ]);
    }

    #[test]
    fn test_quote() {
        assert_eq!(quote(""), "''");
        assert_eq!(quote("'"), "''\\'''");
        assert_eq!(quote("abc"), "abc");
        assert_eq!(quote("a \n  b"), "'a \n  b'");
        assert_eq!(quote("X'\nY"), "'X'\\''\nY'");
    }

    #[test]
    fn test_join() {
        assert_eq!(join(["a", "b", "c"]), "a b c");
        assert_eq!(join([" ", "$", "\n"]), "' ' '$' '\n'");
    }

    #[test]
    fn join_followed_by_split_is_identity() {
        let cases: Vec<&[&str]> = vec![
            &["a"],
            &["python", "-c", "print('Hello world!')"],
            &["echo", " arg with spaces ", "arg \' with \" quotes"],
            &["even newlines are quoted correctly\n", "\n", "\n\n\t "],
            &["$", "`test`"],
            &["cat", "~user/log*"],
            &["test", "'a \"b", "\"X'"],
            &["empty", "", "", ""],
        ];
        for argv in cases {
            let args = join(argv);
            assert_eq!(split(&args).unwrap(), argv);
        }
    }
}

mod test_raw_string_parser {
    use std::ffi::{OsStr, OsString};

    use env::raw_string_parser;

    #[test]
    fn test_ascii_only_take_one_look_at_correct_data_and_end_behavior() {
        let input = "hello";
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);
        for c in input.chars() {
            assert_eq!(c, uut.get_parser().look_at().unwrap());
            uut.take_one().unwrap();
        }
        assert_eq!(
            uut.get_parser().look_at(),
            Err(raw_string_parser::Error {
                look_at_pos: 5,
                err_type: raw_string_parser::ErrorType::EndOfInput
            })
        );
        uut.take_one().unwrap_err();
        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), input);
        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "");
    }

    #[test]
    fn test_multi_byte_codes_take_one_look_at_correct_data_and_end_behavior() {
        let input = OsString::from("🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉");
        let owl: char = '🦉';
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);
        for _i in 0..3 {
            assert_eq!(uut.get_parser().look_at().unwrap(), owl);
            uut.take_one().unwrap();
            assert_eq!(uut.get_parser().look_at().unwrap(), 'x');
            uut.take_one().unwrap();
        }
        assert_eq!(uut.get_parser().look_at().unwrap(), owl);
        uut.take_one().unwrap();
        assert_eq!(
            uut.get_parser().look_at(),
            Err(raw_string_parser::Error {
                look_at_pos: 43,
                err_type: raw_string_parser::ErrorType::EndOfInput
            })
        );
        uut.take_one().unwrap_err();
        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), input);
        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "");
    }

    #[test]
    fn test_multi_byte_codes_put_one_ascii_start_middle_end_try_invalid_ascii() {
        let input = OsString::from("🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉");
        let owl: char = '🦉';
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);
        uut.put_one_char('a');
        for _i in 0..3 {
            assert_eq!(uut.get_parser().look_at().unwrap(), owl);
            uut.take_one().unwrap();
            uut.put_one_char('a');
            assert_eq!(uut.get_parser().look_at().unwrap(), 'x');
            uut.take_one().unwrap();
            uut.put_one_char('a');
        }
        assert_eq!(uut.get_parser().look_at().unwrap(), owl);
        uut.take_one().unwrap();
        uut.put_one_char(owl);
        uut.put_one_char('a');
        assert_eq!(
            uut.get_parser().look_at(),
            Err(raw_string_parser::Error {
                look_at_pos: 43,
                err_type: raw_string_parser::ErrorType::EndOfInput
            })
        );
        uut.take_one().unwrap_err();
        uut.put_one_char('a');
        uut.take_one().unwrap_err();
        assert_eq!(
            uut.take_collected_output(),
            "a🦉🦉🦉axa🦉🦉axa🦉axa🦉🦉🦉🦉🦉aa"
        );
        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "");
    }

    #[test]
    fn test_multi_byte_codes_skip_one_take_one_skip_until_ascii_char_or_end() {
        let input = OsString::from("🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉");
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);

        uut.skip_one().unwrap(); // skip 🦉🦉🦉
        assert_eq!(uut.get_look_at_pos(), 12);

        uut.skip_one().unwrap(); // skip x
        assert_eq!(uut.get_look_at_pos(), 13);
        uut.take_one().unwrap(); // take 🦉🦉
        assert_eq!(uut.get_look_at_pos(), 21);

        uut.skip_one().unwrap(); // skip x
        assert_eq!(uut.get_look_at_pos(), 22);
        uut.get_parser_mut().skip_until_ascii_char_or_end('x'); // skip 🦉
        assert_eq!(uut.get_look_at_pos(), 26);
        uut.take_one().unwrap(); // take x
        uut.get_parser_mut().skip_until_ascii_char_or_end('x'); // skip 🦉🦉🦉🦉 till end
        assert_eq!(uut.get_look_at_pos(), 43);

        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "🦉🦉x");
    }

    #[test]
    fn test_multi_byte_codes_skip_multiple_ascii_bounded_good_and_bad() {
        let input = OsString::from("🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉");
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);

        uut.get_parser_mut().skip_multiple_ascii_bounded(0);
        assert_eq!(uut.get_look_at_pos(), 0);
        uut.get_parser_mut().skip_multiple_ascii_bounded(12); // skips 🦉🦉🦉
        assert_eq!(uut.get_look_at_pos(), 12);

        uut.take_one().unwrap(); // take x
        assert_eq!(uut.get_look_at_pos(), 13);
        uut.get_parser_mut().skip_multiple_ascii_bounded(13); // skips 🦉🦉x🦉
        assert_eq!(uut.get_look_at_pos(), 26);
        uut.take_one().unwrap(); // take x

        assert_eq!(uut.get_look_at_pos(), 27);
        uut.get_parser_mut().skip_multiple_ascii_bounded(16); // skips 🦉🦉🦉🦉
        assert_eq!(uut.get_look_at_pos(), 43);

        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "xx");
    }

    #[test]
    fn test_multi_byte_codes_put_string_utf8_start_middle_end() {
        let input = OsString::from("🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉");
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);

        uut.put_string("🦔oo");
        uut.take_one().unwrap(); // takes 🦉🦉🦉
        uut.put_string("oo🦔");
        uut.take_one().unwrap(); // take x
        uut.get_parser_mut().skip_until_ascii_char_or_end('\n'); // skips till end
        uut.put_string("o🦔o");

        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "🦔oo🦉🦉🦉oo🦔xo🦔o");
    }

    #[test]
    fn test_multi_byte_codes_look_at_remaining_start_middle_end() {
        let input = "🦉🦉🦉x🦉🦉x🦉x🦉🦉🦉🦉";
        let mut uut = env::raw_string_parser::RawStringExpander::new(&input);

        assert_eq!(uut.get_parser().look_at_remaining(), input);
        uut.take_one().unwrap(); // takes 🦉🦉🦉
        assert_eq!(
            uut.get_parser().look_at_remaining(),
            OsStr::new(&input[12..])
        );
        uut.get_parser_mut().skip_until_ascii_char_or_end('\n'); // skips till end
        assert_eq!(uut.get_parser().look_at_remaining(), "");

        uut.take_one().unwrap_err();
        assert_eq!(uut.take_collected_output(), "🦉🦉🦉");
    }
}
