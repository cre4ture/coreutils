// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use du::physical_extents::{Range, SeenPhysicalExtents};
use pretty_assertions::assert_eq;
use std::io::Write;

// spell-checker:ignore (paths) sublink subwords azerty azeaze xcwww azeaz amaz azea qzerty tazerty tsublink testfile1 testfile2 filelist testdir testfile reflink rootdir
#[cfg(not(windows))]
use regex::Regex;
use std::os::unix::fs::FileExt;
use std::rc::Rc;

#[cfg(any(target_os = "linux", target_os = "android"))]
use crate::common::util::expected_result;
use crate::common::util::AtPath;
use crate::common::util::TestScenario;
use crate::common::util::TESTS_BINARY;

extern crate alloc;

const SUB_DIR: &str = "subdir/deeper";
const SUB_DEEPER_DIR: &str = "subdir/deeper/deeper_dir";
const SUB_DIR_LINKS: &str = "subdir/links";
const SUB_DIR_LINKS_DEEPER_SYM_DIR: &str = "subdir/links/deeper_dir";
const SUB_FILE: &str = "subdir/links/subwords.txt";
const SUB_LINK: &str = "subdir/links/sublink.txt";

#[test]
fn test_du_basics() {
    let ts = TestScenario::new(util_name!());

    let result = ts.ucmd().succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    _du_basics(result.stdout_str());
}

#[cfg(target_vendor = "apple")]
fn _du_basics(s: &str) {
    let answer = concat!(
        "4\t./subdir/deeper/deeper_dir\n",
        "8\t./subdir/deeper\n",
        "12\t./subdir/links\n",
        "20\t./subdir\n",
        "24\t.\n"
    );
    assert_eq!(s, answer);
}

#[cfg(target_os = "windows")]
fn _du_basics(s: &str) {
    let answer = concat!(
        "0\t.\\subdir\\deeper\\deeper_dir\n",
        "0\t.\\subdir\\deeper\n",
        "8\t.\\subdir\\links\n",
        "8\t.\\subdir\n",
        "8\t.\n"
    );
    assert_eq!(s, answer);
}

#[cfg(all(not(target_vendor = "apple"), not(target_os = "windows"),))]
fn _du_basics(s: &str) {
    let answer = concat!(
        "8\t./subdir/deeper/deeper_dir\n",
        "16\t./subdir/deeper\n",
        "16\t./subdir/links\n",
        "36\t./subdir\n",
        "44\t.\n"
    );
    assert_eq!(s, answer);
}

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(1);
}

#[test]
fn test_du_basics_subdir() {
    let ts = TestScenario::new(util_name!());

    let result = ts.ucmd().arg(SUB_DIR).succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[SUB_DIR]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    _du_basics_subdir(result.stdout_str());
}

#[cfg(target_vendor = "apple")]
fn _du_basics_subdir(s: &str) {
    assert_eq!(s, "4\tsubdir/deeper/deeper_dir\n8\tsubdir/deeper\n");
}
#[cfg(target_os = "windows")]
fn _du_basics_subdir(s: &str) {
    assert_eq!(s, "0\tsubdir/deeper\\deeper_dir\n0\tsubdir/deeper\n");
}
#[cfg(target_os = "freebsd")]
fn _du_basics_subdir(s: &str) {
    assert_eq!(s, "8\tsubdir/deeper/deeper_dir\n16\tsubdir/deeper\n");
}
#[cfg(all(
    not(target_vendor = "apple"),
    not(target_os = "windows"),
    not(target_os = "freebsd")
))]
fn _du_basics_subdir(s: &str) {
    // MS-WSL linux has altered expected output
    if uucore::os::is_wsl_1() {
        assert_eq!(s, "0\tsubdir/deeper\n");
    } else {
        assert_eq!(s, "8\tsubdir/deeper\n");
    }
}

#[test]
fn test_du_invalid_size() {
    let args = &["block-size", "threshold"];
    let ts = TestScenario::new(util_name!());
    for s in args {
        ts.ucmd()
            .arg(format!("--{s}=1fb4t"))
            .arg("/tmp")
            .fails()
            .code_is(1)
            .stderr_only(format!("du: invalid suffix in --{s} argument '1fb4t'\n"));
        ts.ucmd()
            .arg(format!("--{s}=x"))
            .arg("/tmp")
            .fails()
            .code_is(1)
            .stderr_only(format!("du: invalid --{s} argument 'x'\n"));
        #[cfg(not(target_pointer_width = "128"))]
        ts.ucmd()
            .arg(format!("--{s}=1Y"))
            .arg("/tmp")
            .fails()
            .code_is(1)
            .stderr_only(format!("du: --{s} argument '1Y' too large\n"));
    }
}

#[test]
fn test_du_with_posixly_correct() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;
    let dir = "a";

    at.mkdir(dir);
    at.write(&format!("{dir}/file"), "some content");

    let expected = ts
        .ucmd()
        .arg(dir)
        .arg("--block-size=512")
        .succeeds()
        .stdout_move_str();

    let result = ts
        .ucmd()
        .arg(dir)
        .env("POSIXLY_CORRECT", "1")
        .succeeds()
        .stdout_move_str();

    assert_eq!(expected, result);
}

#[test]
fn test_du_non_existing_files() {
    new_ucmd!()
        .arg("non_existing_a")
        .arg("non_existing_b")
        .fails()
        .stderr_only(concat!(
            "du: cannot access 'non_existing_a': No such file or directory\n",
            "du: cannot access 'non_existing_b': No such file or directory\n"
        ));
}

#[test]
fn test_du_soft_link_all() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_file(SUB_FILE, SUB_LINK);

    let result = ts.ucmd().args(&[SUB_DIR_LINKS, "--all"]).succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[SUB_DIR_LINKS, "--all"]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
}

#[test]
fn test_du_soft_link() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_file(SUB_FILE, SUB_LINK);

    let result = ts.ucmd().arg(SUB_DIR_LINKS).succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[SUB_DIR_LINKS]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    _du_soft_link(result.stdout_str());
}

#[cfg(target_vendor = "apple")]
fn _du_soft_link(s: &str) {
    // 'macos' host variants may have `du` output variation for soft links
    assert!((s == "12\tsubdir/links\n") || (s == "16\tsubdir/links\n"));
}
#[cfg(target_os = "windows")]
fn _du_soft_link(s: &str) {
    assert_eq!(s, "8\tsubdir/links\n");
}
#[cfg(target_os = "freebsd")]
fn _du_soft_link(s: &str) {
    assert_eq!(s, "16\tsubdir/links\n");
}
#[cfg(all(
    not(target_vendor = "apple"),
    not(target_os = "windows"),
    not(target_os = "freebsd")
))]
fn _du_soft_link(s: &str) {
    // MS-WSL linux has altered expected output
    if uucore::os::is_wsl_1() {
        assert_eq!(s, "8\tsubdir/links\n");
    } else {
        assert_eq!(s, "16\tsubdir/links\n");
    }
}

#[cfg(not(target_os = "android"))]
#[test]
fn test_du_hard_link() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.hard_link(SUB_FILE, SUB_LINK);

    let result = ts.ucmd().arg(SUB_DIR_LINKS).succeeds();

    #[cfg(target_os = "linux")]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[SUB_DIR_LINKS]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    // We do not double count hard links as the inodes are identical
    _du_hard_link(result.stdout_str());
}

#[cfg(target_vendor = "apple")]
fn _du_hard_link(s: &str) {
    assert_eq!(s, "12\tsubdir/links\n");
}
#[cfg(target_os = "windows")]
fn _du_hard_link(s: &str) {
    assert_eq!(s, "8\tsubdir/links\n");
}
#[cfg(target_os = "freebsd")]
fn _du_hard_link(s: &str) {
    assert_eq!(s, "16\tsubdir/links\n");
}
#[cfg(all(
    not(target_vendor = "apple"),
    not(target_os = "windows"),
    not(target_os = "freebsd")
))]
fn _du_hard_link(s: &str) {
    // MS-WSL linux has altered expected output
    if uucore::os::is_wsl_1() {
        assert_eq!(s, "8\tsubdir/links\n");
    } else {
        assert_eq!(s, "16\tsubdir/links\n");
    }
}

#[test]
fn test_du_d_flag() {
    let ts = TestScenario::new(util_name!());

    let result = ts.ucmd().arg("-d1").succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &["-d1"]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    _du_d_flag(result.stdout_str());
}

#[cfg(target_vendor = "apple")]
fn _du_d_flag(s: &str) {
    assert_eq!(s, "20\t./subdir\n24\t.\n");
}
#[cfg(target_os = "windows")]
fn _du_d_flag(s: &str) {
    assert_eq!(s, "8\t.\\subdir\n8\t.\n");
}
#[cfg(target_os = "freebsd")]
fn _du_d_flag(s: &str) {
    assert_eq!(s, "36\t./subdir\n44\t.\n");
}
#[cfg(all(
    not(target_vendor = "apple"),
    not(target_os = "windows"),
    not(target_os = "freebsd")
))]
fn _du_d_flag(s: &str) {
    // MS-WSL linux has altered expected output
    if uucore::os::is_wsl_1() {
        assert_eq!(s, "8\t./subdir\n8\t.\n");
    } else {
        assert_eq!(s, "28\t./subdir\n36\t.\n");
    }
}

#[test]
fn test_du_dereference() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_dir(SUB_DEEPER_DIR, SUB_DIR_LINKS_DEEPER_SYM_DIR);

    let result = ts.ucmd().arg("-L").arg(SUB_DIR_LINKS).succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &["-L", SUB_DIR_LINKS]));

        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }

    _du_dereference(result.stdout_str());
}

#[cfg(not(windows))]
#[test]
fn test_du_dereference_args() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir("dir");
    at.write("dir/file-ignore1", "azeaze");
    at.write("dir/file-ignore2", "amaz?ng");
    at.symlink_dir("dir", "sublink");

    for arg in ["-D", "-H", "--dereference-args"] {
        let result = ts.ucmd().arg(arg).arg("-s").arg("sublink").succeeds();
        let stdout = result.stdout_str();

        assert!(!stdout.starts_with('0'));
        assert!(stdout.contains("sublink"));
    }

    // Without the option
    let result = ts.ucmd().arg("-s").arg("sublink").succeeds();
    result.stdout_contains("0\tsublink\n");
}

#[cfg(target_vendor = "apple")]
fn _du_dereference(s: &str) {
    assert_eq!(s, "4\tsubdir/links/deeper_dir\n16\tsubdir/links\n");
}
#[cfg(target_os = "windows")]
fn _du_dereference(s: &str) {
    assert_eq!(s, "0\tsubdir/links\\deeper_dir\n8\tsubdir/links\n");
}
#[cfg(target_os = "freebsd")]
fn _du_dereference(s: &str) {
    assert_eq!(s, "8\tsubdir/links/deeper_dir\n24\tsubdir/links\n");
}
#[cfg(all(
    not(target_vendor = "apple"),
    not(target_os = "windows"),
    not(target_os = "freebsd")
))]
fn _du_dereference(s: &str) {
    // MS-WSL linux has altered expected output
    if uucore::os::is_wsl_1() {
        assert_eq!(s, "0\tsubdir/links/deeper_dir\n8\tsubdir/links\n");
    } else {
        assert_eq!(s, "8\tsubdir/links/deeper_dir\n24\tsubdir/links\n");
    }
}

#[cfg(not(any(target_os = "windows", target_os = "android", target_os = "freebsd")))]
#[test]
fn test_du_no_dereference() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;
    let dir = "a_dir";
    let symlink = "symlink";

    at.mkdir(dir);
    at.symlink_dir(dir, symlink);

    for arg in ["-P", "--no-dereference"] {
        ts.ucmd()
            .arg(arg)
            .succeeds()
            .stdout_contains(dir)
            .stdout_does_not_contain(symlink);

        // ensure no-dereference "wins"
        ts.ucmd()
            .arg("--dereference")
            .arg(arg)
            .succeeds()
            .stdout_contains(dir)
            .stdout_does_not_contain(symlink);

        // ensure dereference "wins"
        let result = ts.ucmd().arg(arg).arg("--dereference").succeeds();

        #[cfg(target_os = "linux")]
        {
            let result_reference = unwrap_or_return!(expected_result(&ts, &[arg, "--dereference"]));

            if result_reference.succeeded() {
                assert_eq!(result.stdout_str(), result_reference.stdout_str());
            }
        }

        #[cfg(not(target_os = "linux"))]
        result.stdout_contains(symlink).stdout_does_not_contain(dir);
    }
}

#[test]
fn test_du_inodes_basic() {
    let ts = TestScenario::new(util_name!());
    let result = ts.ucmd().arg("--inodes").succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &["--inodes"]));
        assert_eq!(result.stdout_str(), result_reference.stdout_str());
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    _du_inodes_basic(result.stdout_str());
}

#[cfg(target_os = "windows")]
fn _du_inodes_basic(s: &str) {
    assert_eq!(
        s,
        concat!(
            "2\t.\\subdir\\deeper\\deeper_dir\n",
            "4\t.\\subdir\\deeper\n",
            "3\t.\\subdir\\links\n",
            "8\t.\\subdir\n",
            "11\t.\n",
        )
    );
}

#[cfg(not(target_os = "windows"))]
fn _du_inodes_basic(s: &str) {
    assert_eq!(
        s,
        concat!(
            "2\t./subdir/deeper/deeper_dir\n",
            "4\t./subdir/deeper\n",
            "3\t./subdir/links\n",
            "8\t./subdir\n",
            "11\t.\n",
        )
    );
}

#[test]
fn test_du_inodes() {
    let ts = TestScenario::new(util_name!());

    ts.ucmd()
        .arg("--summarize")
        .arg("--inodes")
        .succeeds()
        .stdout_only("11\t.\n");

    let result = ts.ucmd().arg("--separate-dirs").arg("--inodes").succeeds();

    #[cfg(target_os = "windows")]
    result.stdout_contains("3\t.\\subdir\\links\n");
    #[cfg(not(target_os = "windows"))]
    result.stdout_contains("3\t./subdir/links\n");
    result.stdout_contains("3\t.\n");

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference =
            unwrap_or_return!(expected_result(&ts, &["--separate-dirs", "--inodes"]));
        assert_eq!(result.stdout_str(), result_reference.stdout_str());
    }
}

#[cfg(not(target_os = "android"))]
#[test]
fn test_du_inodes_with_count_links() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir("dir");
    at.touch("dir/file");
    at.hard_link("dir/file", "dir/hard_link_a");
    at.hard_link("dir/file", "dir/hard_link_b");

    // ensure the hard links are not counted without --count-links
    ts.ucmd()
        .arg("--inodes")
        .arg("dir")
        .succeeds()
        .stdout_is("2\tdir\n");

    for arg in ["-l", "--count-links"] {
        ts.ucmd()
            .arg("--inodes")
            .arg(arg)
            .arg("dir")
            .succeeds()
            .stdout_is("4\tdir\n");
    }
}

#[test]
fn test_du_h_flag_empty_file() {
    new_ucmd!()
        .arg("-h")
        .arg("empty.txt")
        .succeeds()
        .stdout_only("0\tempty.txt\n");
}

#[cfg(feature = "touch")]
#[test]
fn test_du_time() {
    let ts = TestScenario::new(util_name!());

    // du --time formats the timestamp according to the local timezone. We set the TZ
    // environment variable to UTC in the commands below to ensure consistent outputs
    // and test results regardless of the timezone of the machine this test runs in.

    ts.ccmd("touch")
        .env("TZ", "UTC")
        .arg("-a")
        .arg("-t")
        .arg("201505150000")
        .arg("date_test")
        .succeeds();

    ts.ccmd("touch")
        .env("TZ", "UTC")
        .arg("-m")
        .arg("-t")
        .arg("201606160000")
        .arg("date_test")
        .succeeds();

    let result = ts
        .ucmd()
        .env("TZ", "UTC")
        .arg("--time")
        .arg("date_test")
        .succeeds();
    result.stdout_only("0\t2016-06-16 00:00\tdate_test\n");

    let result = ts
        .ucmd()
        .env("TZ", "UTC")
        .arg("--time=atime")
        .arg("date_test")
        .succeeds();
    result.stdout_only("0\t2015-05-15 00:00\tdate_test\n");

    let result = ts
        .ucmd()
        .env("TZ", "UTC")
        .arg("--time=ctime")
        .arg("date_test")
        .succeeds();
    result.stdout_only("0\t2016-06-16 00:00\tdate_test\n");

    if birth_supported() {
        use regex::Regex;

        let re_birth =
            Regex::new(r"0\t[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}\tdate_test").unwrap();
        let result = ts.ucmd().arg("--time=birth").arg("date_test").succeeds();
        result.stdout_matches(&re_birth);
    }
}

#[cfg(feature = "touch")]
fn birth_supported() -> bool {
    let ts = TestScenario::new(util_name!());
    let m = match std::fs::metadata(ts.fixtures.subdir) {
        Ok(m) => m,
        Err(e) => panic!("{}", e),
    };
    m.created().is_ok()
}

#[cfg(not(target_os = "windows"))]
#[cfg(feature = "chmod")]
#[test]
fn test_du_no_permission() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all(SUB_DIR_LINKS);

    ts.ccmd("chmod").arg("-r").arg(SUB_DIR_LINKS).succeeds();

    let result = ts.ucmd().arg(SUB_DIR_LINKS).fails();
    result.stderr_contains("du: cannot read directory 'subdir/links': Permission denied");

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &[SUB_DIR_LINKS]));
        if result_reference
            .stderr_str()
            .contains("du: cannot read directory 'subdir/links': Permission denied")
        {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }

    _du_no_permission(result.stdout_str());
}

#[cfg(not(target_os = "windows"))]
#[cfg(feature = "chmod")]
#[test]
fn test_du_no_exec_permission() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all("d/no-x/y");

    ts.ccmd("chmod").arg("u=rw").arg("d/no-x").succeeds();

    let result = ts.ucmd().arg("d/no-x").fails();
    result.stderr_contains("du: cannot access 'd/no-x/y': Permission denied");
}

#[cfg(target_vendor = "apple")]
fn _du_no_permission(s: &str) {
    assert_eq!(s, "0\tsubdir/links\n");
}
#[cfg(all(not(target_vendor = "apple"), not(target_os = "windows")))]
fn _du_no_permission(s: &str) {
    assert_eq!(s, "4\tsubdir/links\n");
}

#[test]
fn test_du_one_file_system() {
    let ts = TestScenario::new(util_name!());

    let result = ts.ucmd().arg("-x").arg(SUB_DIR).succeeds();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let result_reference = unwrap_or_return!(expected_result(&ts, &["-x", SUB_DIR]));
        if result_reference.succeeded() {
            assert_eq!(result.stdout_str(), result_reference.stdout_str());
            return;
        }
    }
    _du_basics_subdir(result.stdout_str());
}

#[test]
fn test_du_threshold() {
    let ts = TestScenario::new(util_name!());

    let threshold = if cfg!(windows) { "7K" } else { "10K" };

    ts.ucmd()
        .arg(format!("--threshold={threshold}"))
        .succeeds()
        .stdout_contains("links")
        .stdout_does_not_contain("deeper_dir");

    ts.ucmd()
        .arg(format!("--threshold=-{threshold}"))
        .succeeds()
        .stdout_does_not_contain("links")
        .stdout_contains("deeper_dir");
}

#[test]
fn test_du_invalid_threshold() {
    let ts = TestScenario::new(util_name!());

    let threshold = "-0";

    ts.ucmd().arg(format!("--threshold={threshold}")).fails();
}

#[test]
fn test_du_apparent_size() {
    let (at, mut ucmd) = at_and_ucmd!();

    at.mkdir_all("a/b");

    at.write("a/b/file1", "foo");
    at.write("a/b/file2", "foobar");

    let result = ucmd.args(&["--apparent-size", "--all", "a"]).succeeds();

    #[cfg(not(target_os = "windows"))]
    {
        result.stdout_contains_line("1\ta/b/file2");
        result.stdout_contains_line("1\ta/b/file1");
        result.stdout_contains_line("1\ta/b");
        result.stdout_contains_line("1\ta");
    }

    #[cfg(target_os = "windows")]
    {
        result.stdout_contains_line("1\ta\\b\\file2");
        result.stdout_contains_line("1\ta\\b\\file1");
        result.stdout_contains_line("1\ta\\b");
        result.stdout_contains_line("1\ta");
    }
}

#[test]
fn test_du_bytes() {
    let (at, mut ucmd) = at_and_ucmd!();

    at.mkdir_all("a/b");

    at.write("a/b/file1", "foo");
    at.write("a/b/file2", "foobar");

    let result = ucmd.args(&["--bytes", "--all", "a"]).succeeds();

    #[cfg(not(target_os = "windows"))]
    {
        result.stdout_contains_line("6\ta/b/file2");
        result.stdout_contains_line("3\ta/b/file1");
        result.stdout_contains_line("9\ta/b");
        result.stdout_contains_line("9\ta");
    }

    #[cfg(target_os = "windows")]
    {
        result.stdout_contains_line("6\ta\\b\\file2");
        result.stdout_contains_line("3\ta\\b\\file1");
        result.stdout_contains_line("9\ta\\b");
        result.stdout_contains_line("9\ta");
    }
}

#[test]
fn test_du_exclude() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_dir(SUB_DEEPER_DIR, SUB_DIR_LINKS_DEEPER_SYM_DIR);
    at.mkdir_all(SUB_DIR_LINKS);

    ts.ucmd()
        .arg("--exclude=subdir")
        .arg(SUB_DEEPER_DIR)
        .succeeds()
        .stdout_contains("subdir/deeper/deeper_dir");
    ts.ucmd()
        .arg("--exclude=subdir")
        .arg("subdir")
        .succeeds()
        .stdout_is("");
    ts.ucmd()
        .arg("--exclude=subdir")
        .arg("--verbose")
        .arg("subdir")
        .succeeds()
        .stdout_contains("'subdir' ignored");
}

#[test]
// Disable on Windows because we are looking for /
// And the tests would be more complex if we have to support \ too
#[cfg(not(target_os = "windows"))]
fn test_du_exclude_2() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all("azerty/xcwww/azeaze");

    let result = ts.ucmd().arg("azerty").succeeds();

    let path_regexp = r"(.*)azerty/xcwww/azeaze(.*)azerty/xcwww(.*)azerty";
    let re = Regex::new(path_regexp).unwrap();
    assert!(re.is_match(result.stdout_str().replace('\n', "").trim()));

    // Exact match
    ts.ucmd()
        .arg("--exclude=azeaze")
        .arg("azerty")
        .succeeds()
        .stdout_does_not_contain("azerty/xcwww/azeaze");
    // Partial match and NOT a glob
    ts.ucmd()
        .arg("--exclude=azeaz")
        .arg("azerty")
        .succeeds()
        .stdout_contains("azerty/xcwww/azeaze");
    // Partial match and a various glob
    ts.ucmd()
        .arg("--exclude=azea?")
        .arg("azerty")
        .succeeds()
        .stdout_contains("azerty/xcwww/azeaze");
    ts.ucmd()
        .arg("--exclude=azea{z,b}")
        .arg("azerty")
        .succeeds()
        .stdout_contains("azerty/xcwww/azeaze");
    ts.ucmd()
        .arg("--exclude=azea*")
        .arg("azerty")
        .succeeds()
        .stdout_does_not_contain("azerty/xcwww/azeaze");
    ts.ucmd()
        .arg("--exclude=azeaz?")
        .arg("azerty")
        .succeeds()
        .stdout_does_not_contain("azerty/xcwww/azeaze");
}

#[test]
// Disable on Windows because we are looking for /
// And the tests would be more complex if we have to support \ too
#[cfg(not(target_os = "windows"))]
fn test_du_exclude_mix() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.write("file-ignore1", "azeaze");
    at.write("file-ignore2", "amaz?ng");

    at.mkdir_all("azerty/xcwww/azeaze");
    at.mkdir_all("azerty/xcwww/qzerty");
    at.mkdir_all("azerty/xcwww/amazing");

    ts.ucmd()
        .arg("azerty")
        .succeeds()
        .stdout_contains("azerty/xcwww/azeaze");
    ts.ucmd()
        .arg("--exclude=azeaze")
        .arg("azerty")
        .succeeds()
        .stdout_does_not_contain("azerty/xcwww/azeaze");

    // Just exclude one file name
    let result = ts.ucmd().arg("--exclude=qzerty").arg("azerty").succeeds();
    assert!(!result.stdout_str().contains("qzerty"));
    assert!(result.stdout_str().contains("azerty"));
    assert!(result.stdout_str().contains("xcwww"));

    // Exclude from file
    let result = ts
        .ucmd()
        .arg("--exclude-from=file-ignore1")
        .arg("azerty")
        .succeeds();
    assert!(!result.stdout_str().contains("azeaze"));
    assert!(result.stdout_str().contains("qzerty"));
    assert!(result.stdout_str().contains("xcwww"));

    // Mix two files and string
    let result = ts
        .ucmd()
        .arg("--exclude=qzerty")
        .arg("--exclude-from=file-ignore1")
        .arg("--exclude-from=file-ignore2")
        .arg("azerty")
        .succeeds();
    assert!(!result.stdout_str().contains("amazing"));
    assert!(!result.stdout_str().contains("qzerty"));
    assert!(!result.stdout_str().contains("azeaze"));
    assert!(result.stdout_str().contains("xcwww"));
}

#[test]
// Disable on Windows because we are looking for /
// And the tests would be more complex if we have to support \ too
#[cfg(not(target_os = "windows"))]
fn test_du_complex_exclude_patterns() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all("azerty/xcwww/azeaze");
    at.mkdir_all("azerty/xcwww/qzerty");
    at.mkdir_all("azerty/xcwww/amazing");

    // Negation in glob should work with both ^ and !
    let result = ts
        .ucmd()
        .arg("--exclude=azerty/*/[^q]*")
        .arg("azerty")
        .succeeds();
    assert!(!result.stdout_str().contains("amazing"));
    assert!(result.stdout_str().contains("qzerty"));
    assert!(!result.stdout_str().contains("azeaze"));
    assert!(result.stdout_str().contains("xcwww"));

    let result = ts
        .ucmd()
        .arg("--exclude=azerty/*/[!q]*")
        .arg("azerty")
        .succeeds();
    assert!(!result.stdout_str().contains("amazing"));
    assert!(result.stdout_str().contains("qzerty"));
    assert!(!result.stdout_str().contains("azeaze"));
    assert!(result.stdout_str().contains("xcwww"));
}

#[test]
fn test_du_exclude_several_components() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all("a/b/c");
    at.mkdir_all("a/x/y");
    at.mkdir_all("a/u/y");

    // Exact match
    let result = ts
        .ucmd()
        .arg("--exclude=a/u")
        .arg("--exclude=a/b")
        .arg("a")
        .succeeds();
    assert!(!result.stdout_str().contains("a/u"));
    assert!(!result.stdout_str().contains("a/b"));
}

#[test]
fn test_du_exclude_invalid_syntax() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir_all("azerty/xcwww/azeaze");

    ts.ucmd()
        .arg("--exclude=a[ze")
        .arg("azerty")
        .fails()
        .stderr_contains("du: Invalid exclude syntax");
}

#[cfg(not(windows))]
#[test]
fn test_du_symlink_fail() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_file("non-existing.txt", "target.txt");

    ts.ucmd().arg("-L").arg("target.txt").fails().code_is(1);
}

#[cfg(not(windows))]
#[test]
fn test_du_symlink_multiple_fail() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.symlink_file("non-existing.txt", "target.txt");
    at.write("file1", "azeaze");

    let result = ts.ucmd().arg("-L").arg("target.txt").arg("file1").fails();
    assert_eq!(result.code(), 1);
    result.stdout_contains("4\tfile1\n");
}

#[test]
// Disable on Windows because of different path separators and handling of null characters
#[cfg(not(target_os = "windows"))]
fn test_du_files0_from() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.write("testfile1", "content1");
    at.write("testfile2", "content2");

    at.mkdir("testdir");
    at.write("testdir/testfile3", "content3");

    at.write("filelist", "testfile1\0testfile2\0testdir\0");

    ts.ucmd()
        .arg("--files0-from=filelist")
        .succeeds()
        .stdout_contains("testfile1")
        .stdout_contains("testfile2")
        .stdout_contains("testdir");
}

#[test]
fn test_du_files0_from_ignore_duplicate_file_names() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;
    let file = "testfile";

    at.touch(file);
    at.write("filelist", &format!("{file}\0{file}\0"));

    ts.ucmd()
        .arg("--files0-from=filelist")
        .succeeds()
        .stdout_is(format!("0\t{file}\n"));
}

#[test]
fn test_du_files0_from_with_invalid_zero_length_file_names() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.touch("testfile");

    at.write("filelist", "\0testfile\0\0");

    ts.ucmd()
        .arg("--files0-from=filelist")
        .fails()
        .code_is(1)
        .stdout_contains("testfile")
        .stderr_contains("filelist:1: invalid zero-length file name")
        .stderr_contains("filelist:3: invalid zero-length file name");
}

#[test]
fn test_du_files0_from_stdin() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.write("testfile1", "content1");
    at.write("testfile2", "content2");

    let input = "testfile1\0testfile2\0";

    ts.ucmd()
        .arg("--files0-from=-")
        .pipe_in(input)
        .succeeds()
        .stdout_contains("testfile1")
        .stdout_contains("testfile2");
}

#[test]
fn test_du_files0_from_stdin_ignore_duplicate_file_names() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;
    let file = "testfile";

    at.touch(file);

    let input = format!("{file}\0{file}");

    ts.ucmd()
        .arg("--files0-from=-")
        .pipe_in(input)
        .succeeds()
        .stdout_is(format!("0\t{file}\n"));
}

#[test]
fn test_du_files0_from_stdin_with_invalid_zero_length_file_names() {
    new_ucmd!()
        .arg("--files0-from=-")
        .pipe_in("\0\0")
        .fails()
        .code_is(1)
        .stderr_contains("-:1: invalid zero-length file name")
        .stderr_contains("-:2: invalid zero-length file name");
}

#[test]
fn test_du_files0_from_dir() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir("dir");

    let result = ts.ucmd().arg("--files0-from=dir").fails();
    assert_eq!(result.stderr_str(), "du: dir: read error: Is a directory\n");
}

#[test]
fn test_du_files0_from_combined() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    at.mkdir("dir");

    let result = ts.ucmd().arg("--files0-from=-").arg("foo").fails();
    let stderr = result.stderr_str();

    assert!(stderr.contains("file operands cannot be combined with --files0-from"));
}

#[cfg(target_os = "freebsd")] // freebsd has zfs filesystem which is copy-on-write
fn create_binary_file(at: &AtPath, size_in_bytes: u64, filename: &str) {
    let mut file = at.make_file(filename);
    for _ in 0..size_in_bytes / 10 {
        file.write_all(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();
    }
    let remaining =
        &([0, 1, 2, 3, 4, 5, 6, 7, 8, 9][..usize::try_from(size_in_bytes % 10).unwrap()]);
    file.write_all(&remaining).unwrap();
    file.flush().unwrap();
}

#[cfg(target_os = "freebsd")] // freebsd has zfs filesystem which is copy-on-write
#[test]
fn test_du_reflink_copy_not_considered_as_extra_data() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    create_binary_file(&at, 10 * 1024 * 1024, "large_file1.bin");

    ts.cmd("cp")
        .arg("--reflink=always")
        .arg("large_file1.bin")
        .arg("large_file1_cp_reflink_always.bin")
        .succeeds();

    let result = ts.ucmd().args(&["--all", "--shared-extents"]).succeeds();
    result.stdout_contains("10240\t./large_file1.bin\n");
    //result.stdout_contains("0\t./large_file1_cp_reflink_always.bin\n");
    result.stdout_contains("10240\t.\n");

    // same, but with disabled extend searching feature:
    let result = ts.ucmd().args(&["--all"]).succeeds();
    result.stdout_contains("10240\t./large_file1.bin\n");
    result.stdout_contains("10240\t./large_file1_cp_reflink_always.bin\n");
    result.stdout_contains("20480\t.\n");
}

#[cfg(target_os = "freebsd")] // freebsd has zfs filesystem which is copy-on-write
#[test]
fn test_du_reflink_copy_of_very_small_files_considered_as_extra_data_as_its_part_of_metadata() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    create_binary_file(&at, 50, "small_file1.bin");

    ts.cmd("cp")
        .arg("--reflink=always")
        .arg("small_file1.bin")
        .arg("small_file1_cp_reflink_always.bin")
        .succeeds();

    let result = ts
        .ucmd()
        .args(&["--all", "-b", "--shared-extents"])
        .succeeds();
    result.stdout_contains("50\t./small_file1.bin\n");
    result.stdout_contains("50\t./small_file1_cp_reflink_always.bin\n");
    result.stdout_contains("100\t.\n");
}

#[cfg(target_os = "freebsd")] // freebsd has zfs filesystem which is copy-on-write
#[test]
fn test_du_reflink_partial_copy_not_considered_as_extra_data() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    create_binary_file(&at, 10 * 1024 * 1024, "large_file1.bin");

    ts.cmd("cp")
        .arg("--reflink=always")
        .arg("large_file1.bin")
        .arg("large_file1_cp_reflink_always_partial.bin")
        .succeeds();

    let mut f = at.open_read_write("large_file1_cp_reflink_always_partial.bin");
    f.write_at(&[99; 1000 * 1024], 1024 * 1024).unwrap();
    f.flush().unwrap();
    drop(f);

    // force sync on filesystem, otherwise modifications are still pending
    ts.cmd("sync").args(&["-f", "."]).succeeds();

    let result = ts.ucmd().args(&["--all", "--shared-extents"]).succeeds();
    result.stdout_contains("10240\t./large_file1.bin\n");
    result.stdout_contains("1000\t./large_file1_cp_reflink_always_partial.bin\n");
    result.stdout_contains("11240\t.\n");
}

#[cfg(target_os = "freebsd")] // freebsd has zfs filesystem which is copy-on-write
#[test]
fn test_du_symlink_to_reflink_copy_do_not_omit_symlink_printing() {
    let ts = TestScenario::new(util_name!());
    let at = &ts.fixtures;

    create_binary_file(&at, 1024 * 1024, "large_file1.bin");

    ts.cmd("cp")
        .arg("--reflink=always")
        .arg("large_file1.bin")
        .arg("large_file1_cp_reflink_always.bin")
        .succeeds();

    at.symlink_file("large_file1.bin", "large_file1_symlink.bin");

    let result = ts.ucmd().args(&["--all", "--shared-extents"]).succeeds();
    result.stdout_contains("1024\t./large_file1.bin\n");
    result.stdout_contains("4\t./large_file1_symlink.bin\n");
    //result.stdout_contains("0\t./large_file1_cp_reflink_always.bin\n");
    result.stdout_contains("1028\t.\n");
}

#[test]
fn test_du_overlapping_ranges() {
    //let ts = TestScenario::new(util_name!());
    //let at = &ts.fixtures;

    let mut uut = SeenPhysicalExtents::default();

    assert_eq!(uut.ranges.len(), 0);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range { start: 0, end: 100 }),
        0
    );
    assert_eq!(uut.ranges.len(), 1);
    assert_eq!(*uut.ranges.entry(0).or_default(), 100);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 400,
            end: 500
        }),
        0
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(0).or_default(), 100);
    assert_eq!(*uut.ranges.entry(400).or_default(), 500);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range { start: 0, end: 100 }),
        100
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 400,
            end: 500
        }),
        100
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 600,
            end: 700
        }),
        0
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(*uut.ranges.entry(0).or_default(), 100);
    assert_eq!(*uut.ranges.entry(400).or_default(), 500);
    assert_eq!(*uut.ranges.entry(600).or_default(), 700);

    assert_eq!(
        uut.get_overlapping_and_insert(&Range { start: 0, end: 50 }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 400,
            end: 450
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 600,
            end: 650
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);

    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 50,
            end: 100
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 450,
            end: 500
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 650,
            end: 700
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
}

#[test]
fn test_du_overlapping_ranges_and_extending() {
    //let ts = TestScenario::new(util_name!());
    //let at = &ts.fixtures;

    let mut uut = SeenPhysicalExtents::default();

    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 50,
            end: 150
        }),
        0
    );
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 400,
            end: 500
        }),
        0
    );
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 600,
            end: 700
        }),
        0
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(*uut.ranges.entry(50).or_default(), 150);
    assert_eq!(*uut.ranges.entry(400).or_default(), 500);
    assert_eq!(*uut.ranges.entry(600).or_default(), 700);

    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 25,
            end: 100
        }),
        50
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(*uut.ranges.entry(25).or_default(), 150);
    assert_eq!(*uut.ranges.entry(400).or_default(), 500);
    assert_eq!(*uut.ranges.entry(600).or_default(), 700);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 125,
            end: 200
        }),
        25
    );
    assert_eq!(uut.ranges.len(), 3);
    assert_eq!(*uut.ranges.entry(25).or_default(), 200);
    assert_eq!(*uut.ranges.entry(400).or_default(), 500);
    assert_eq!(*uut.ranges.entry(600).or_default(), 700);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 390,
            end: 800
        }),
        200
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 200);
    assert_eq!(*uut.ranges.entry(390).or_default(), 800);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 200,
            end: 210
        }),
        0
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 210);
    assert_eq!(*uut.ranges.entry(390).or_default(), 800);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 380,
            end: 390
        }),
        0
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 210);
    assert_eq!(*uut.ranges.entry(380).or_default(), 800);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range { start: 25, end: 50 }),
        25
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 210);
    assert_eq!(*uut.ranges.entry(380).or_default(), 800);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 100,
            end: 210
        }),
        110
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 210);
    assert_eq!(*uut.ranges.entry(380).or_default(), 800);
    assert_eq!(
        uut.get_overlapping_and_insert(&Range {
            start: 25,
            end: 210
        }),
        185
    );
    assert_eq!(uut.ranges.len(), 2);
    assert_eq!(*uut.ranges.entry(25).or_default(), 210);
    assert_eq!(*uut.ranges.entry(380).or_default(), 800);
}
