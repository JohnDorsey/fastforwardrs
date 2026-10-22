// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of fetching shaders (phase 6.9): the manifest and its policy, the download with files made on the spot (nothing is downloaded from the network), the checksums, a download
//! that fails or is cancelled, one run of curl for many files against a server that is run here, and the manifest that is kept in the program.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::plugin_bundles::Curl;
use crate::plugin_bundles::test_support::{FakeFetcher, a_shader_fixture, a_shader_that_says, sha};

fn fetch(
    fixture: &super::super::test_support::ShaderFixture,
    fetcher: &FakeFetcher,
    folder: &Path,
) -> (Result<ShaderOutcome, String>, Vec<Progress>) {
    let mut reports = Vec::new();
    let result = fetch_shaders(
        &fixture.manifest,
        folder,
        fetcher,
        &mut |progress| reports.push(progress.clone()),
        &|| false,
    );
    (result, reports)
}

// --- the manifest ---

#[test]
fn the_manifest_in_the_program_is_read_and_is_fit_and_every_file_of_it_is_what_the_policy_allows() {
    let manifest = ShaderManifest::embedded();

    assert_eq!(manifest.problems(), Vec::<String>::new());
    assert!(!manifest.sets.is_empty());
    for set in &manifest.sets {
        assert!(!set.shaders.is_empty(), "{}", set.id);
        for shader in &set.shaders {
            assert!(
                shader.url.starts_with("https://raw.githubusercontent.com/"),
                "{}",
                shader.url
            );
            assert!(
                shader
                    .url
                    .contains(&format!("/{}/", set.source.url.rsplit('/').next().unwrap())),
                "the address is of the commit that the set is of: {}",
                shader.url
            );
        }
    }
}

#[test]
fn the_fixture_is_fit_and_a_manifest_with_a_fault_in_it_says_what_is_wrong_in_one_line_each() {
    let fixture = a_shader_fixture();
    assert_eq!(fixture.manifest.problems(), Vec::<String>::new());

    let broken = |change: &dyn Fn(&mut ShaderManifest)| {
        let mut manifest = fixture.manifest.clone();
        change(&mut manifest);
        manifest.problems()
    };
    let says = |problems: Vec<String>, wanted: &str| {
        assert!(
            problems.iter().any(|problem| problem.contains(wanted)),
            "{wanted:?} in {problems:?}"
        );
    };
    says(broken(&|m| m.format = 2), "the format of the manifest of shaders is 2");
    says(broken(&|m| m.sets.clear()), "has no sets");
    says(
        broken(&|m| m.sets[0].id = "Not Plain".to_owned()),
        "the id is not lower case",
    );
    says(broken(&|m| m.sets[1].id = "pack".to_owned()), "the id is used twice");
    says(
        broken(&|m| m.sets[0].name.clear()),
        "a name, a version, authors and shaders are needed",
    );
    says(
        broken(&|m| m.sets[0].shaders.clear()),
        "a name, a version, authors and shaders are needed",
    );
    says(
        broken(&|m| m.sets[0].homepage = "http://example.org/".to_owned()),
        "the homepage is not an https address",
    );
    says(
        broken(&|m| m.sets[0].licence = "AGPL-3.0-only".to_owned()),
        "is not one that may ship",
    );
    says(broken(&|m| m.sets[0].licence_texts.clear()), "no licence text");
    says(
        broken(&|m| m.sets[0].licence_texts[0].sha256 = "xyz".to_owned()),
        "a licence text needs",
    );
    says(
        broken(&|m| m.sets[0].licence_texts[0].name = "NOTICE.txt".to_owned()),
        "has the name of a file",
    );
    says(
        broken(&|m| m.sets[0].licence_texts[0].name = "Glow.fs".to_owned()),
        "has the name of a file",
    );
    says(
        broken(&|m| {
            let text = m.sets[0].licence_texts[0].clone();
            m.sets[0].licence_texts.push(text);
        }),
        "a licence text name is used twice",
    );
    says(
        broken(&|m| m.sets[0].source.url = "http://example.org/".to_owned()),
        "the source has no https address",
    );
    says(broken(&|m| m.sets[0].shaders[0].name.clear()), "no name");
    says(
        broken(&|m| m.sets[0].shaders[0].file = "dir/Swirl.fs".to_owned()),
        "not a plain name of a .fs or .isf file",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].file = "Swirl.exe".to_owned()),
        "not a plain name of a .fs or .isf file",
    );
    says(
        broken(&|m| m.sets[0].shaders[1].file = "Swirl Thing.fs".to_owned()),
        "the name is used twice",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].url = "http://example.org/a.fs".to_owned()),
        "needs an https address and a sha256",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].sha256 = "ABC".to_owned()),
        "needs an https address and a sha256",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].size = 0),
        "is not a size that a shader can be",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].size = MOST_BYTES_OF_A_SHADER as u64 + 1),
        "is not a size that a shader can be",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].licence = "CC-BY-NC-4.0".to_owned()),
        "is not one that may ship",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].licence = "Apache-2.0".to_owned()),
        "is not among those the set says it has",
    );
    says(
        broken(&|m| m.sets[0].shaders[0].shape = "effect".to_owned()),
        "is not a filter, a generator or a transition",
    );
}

#[test]
fn a_shader_of_exactly_the_most_bytes_is_fit_and_a_licence_of_the_set_that_a_file_has_is_fit() {
    let mut manifest = a_shader_fixture().manifest;
    manifest.sets[0].shaders[0].size = MOST_BYTES_OF_A_SHADER as u64;
    manifest.sets[0].shaders[1].file = "Glow.ISF".to_owned();
    assert_eq!(manifest.problems(), Vec::<String>::new());
}

#[test]
fn a_set_that_is_fetched_has_every_file_of_it_in_its_folder_and_the_bytes_of_the_download_are_those_of_the_shaders() {
    let fixture = a_shader_fixture();
    let folder = tempfile::tempdir().unwrap();
    let pack = &fixture.manifest.sets[0];
    assert!(!shaders_are_fetched(pack, folder.path()));
    std::fs::create_dir_all(folder.path().join("pack")).unwrap();
    std::fs::write(folder.path().join("pack").join("Swirl Thing.fs"), b"a").unwrap();
    assert!(!shaders_are_fetched(pack, folder.path()), "one is not all of them");
    std::fs::write(folder.path().join("pack").join("Glow.fs"), b"b").unwrap();
    assert!(shaders_are_fetched(pack, folder.path()));
    assert_eq!(folder_of_a_shader_set(pack, folder.path()), folder.path().join("pack"));
    let total: u64 = fixture
        .manifest
        .sets
        .iter()
        .flat_map(|set| &set.shaders)
        .map(|shader| shader.size)
        .sum();
    assert_eq!(shader_bytes_to_download(&fixture.manifest), total);
    assert!(total > 0);
}

// --- the download ---

#[test]
fn every_shader_of_every_set_is_fetched_into_the_folder_of_its_set_with_its_licence_text_and_a_notice() {
    let fixture = a_shader_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    let (result, reports) = fetch(&fixture, &fetcher, folder.path());

    let outcome = result.unwrap();
    assert_eq!(outcome.sets, ["PACK", "SOLO"]);
    assert_eq!(outcome.files, 3);
    for set in &fixture.manifest.sets {
        for shader in &set.shaders {
            let wanted = &fixture.files[&shader.url];
            let made = std::fs::read(folder.path().join(&set.id).join(&shader.file)).unwrap();
            assert_eq!(&made, wanted, "{}: the bytes that were fetched, unaltered", shader.file);
        }
        let licence = std::fs::read(folder.path().join(&set.id).join("LICENSE.txt")).unwrap();
        assert_eq!(licence, fixture.files[&set.licence_texts[0].url]);
        let notice = std::fs::read_to_string(folder.path().join(&set.id).join("NOTICE.txt")).unwrap();
        assert_eq!(notice, notice_of_a_shader_set(set));
    }
    // Nothing else is left: no folder of a build or a download.
    let mut left: Vec<String> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["pack", "solo"]);
    // The progress goes from the first set to the end, and ends by saying that it is done.
    assert!(
        reports[0].step.starts_with("Downloading PACK (1 of 2)"),
        "{:?}",
        reports[0]
    );
    assert!(
        reports
            .iter()
            .any(|report| report.step.starts_with("Checking SOLO (2 of 2)"))
    );
    let last = reports.last().unwrap();
    assert_eq!((last.step.as_str(), last.bytes_done), ("Done", last.bytes_total));
    assert_eq!(last.bytes_total, shader_bytes_to_download(&fixture.manifest));
    assert!(
        reports.windows(2).all(|pair| pair[0].bytes_done <= pair[1].bytes_done),
        "never backwards"
    );
    assert!(reports.iter().all(|report| report.bytes_done <= report.bytes_total));
}

#[test]
fn what_is_asked_of_the_fetcher_is_each_address_of_the_manifest_once_and_nothing_else() {
    let fixture = a_shader_fixture();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let folder = tempfile::tempdir().unwrap();

    fetch(&fixture, &fetcher, folder.path()).0.unwrap();

    let mut asked = fetcher.asked();
    asked.sort();
    let mut wanted: Vec<String> = fixture.files.keys().cloned().collect();
    wanted.sort();
    assert_eq!(asked, wanted);
}

#[test]
fn a_file_whose_checksum_is_not_the_one_of_the_manifest_is_not_used_and_says_so_and_nothing_of_the_set_is_put_in_place()
{
    let fixture = a_shader_fixture();
    let mut files = fixture.files.clone();
    let tampered = fixture.manifest.sets[0].shaders[1].url.clone();
    files.insert(
        tampered.clone(),
        b"void main() { /* not what was expected */ }".to_vec(),
    );
    let fetcher = FakeFetcher::new(files);
    let folder = tempfile::tempdir().unwrap();

    let (result, _) = fetch(&fixture, &fetcher, folder.path());

    let error = result.unwrap_err();
    assert!(
        error.contains(&tampered) && error.contains("is not the file that was expected"),
        "{error}"
    );
    assert!(!folder.path().join("pack").exists(), "nothing of the set that failed");
    assert!(!folder.path().join("pack.building").exists());
    assert!(
        !folder.path().join("solo").exists(),
        "and the sets after it were not begun"
    );
}

#[test]
fn a_licence_text_whose_checksum_is_not_the_one_of_the_manifest_is_not_used_either() {
    let fixture = a_shader_fixture();
    let mut files = fixture.files.clone();
    let text = fixture.manifest.sets[0].licence_texts[0].url.clone();
    files.insert(text.clone(), b"Some other licence, not the one that was named".to_vec());
    let folder = tempfile::tempdir().unwrap();

    let (result, _) = fetch(&fixture, &FakeFetcher::new(files), folder.path());

    let error = result.unwrap_err();
    assert!(
        error.contains(&text) && error.contains("is not the file that was expected"),
        "{error}"
    );
    assert!(!folder.path().join("pack").exists());
}

#[test]
fn a_set_that_was_there_before_is_left_as_it_was_when_its_new_version_cannot_be_made_and_replaced_when_it_can() {
    let fixture = a_shader_fixture();
    let folder = tempfile::tempdir().unwrap();
    let before = folder.path().join("pack");
    std::fs::create_dir_all(&before).unwrap();
    std::fs::write(before.join("Old.fs"), b"an old one").unwrap();

    let mut files = fixture.files.clone();
    files.remove(&fixture.manifest.sets[0].shaders[1].url);
    let (result, _) = fetch(&fixture, &FakeFetcher::new(files), folder.path());
    assert!(result.unwrap_err().contains("could not be downloaded"));
    assert_eq!(
        std::fs::read(before.join("Old.fs")).unwrap(),
        b"an old one",
        "as it was"
    );

    let (result, _) = fetch(&fixture, &FakeFetcher::new(fixture.files.clone()), folder.path());
    result.unwrap();
    assert!(!before.join("Old.fs").exists(), "replaced by the new version, whole");
    assert!(before.join("Glow.fs").exists());
}

#[test]
fn a_download_that_is_cancelled_says_so_and_leaves_nothing_and_a_manifest_that_is_not_fit_is_not_begun() {
    let fixture = a_shader_fixture();
    let folder = tempfile::tempdir().unwrap();
    let cancelled = fetch_shaders(
        &fixture.manifest,
        folder.path(),
        &FakeFetcher::new(fixture.files.clone()),
        &mut |_| {},
        &|| true,
    );
    assert_eq!(cancelled.unwrap_err(), CANCELLED);
    assert!(!folder.path().join("pack").exists());

    let mut fetcher = FakeFetcher::new(fixture.files.clone());
    fetcher.cancel_after = Some(1);
    let (result, _) = fetch(&fixture, &fetcher, folder.path());
    assert_eq!(result.unwrap_err(), CANCELLED);
    let left: Vec<_> = std::fs::read_dir(folder.path()).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");

    let mut broken = fixture.manifest.clone();
    broken.sets[0].shaders[0].url = "http://example.org/not-https.fs".to_owned();
    let fetcher = FakeFetcher::new(fixture.files.clone());
    let error = fetch_shaders(&broken, folder.path(), &fetcher, &mut |_| {}, &|| false).unwrap_err();
    assert!(
        error.starts_with("the list of shaders is not fit to be used"),
        "{error}"
    );
    assert!(fetcher.asked().is_empty(), "nothing was asked of the network");
}

#[test]
fn a_fetcher_that_fetches_many_files_one_after_another_leaves_none_of_them_when_one_fails() {
    let fetcher = FakeFetcher::new(HashMap::from([
        ("https://example.org/a".to_owned(), b"aaa".to_vec()),
        ("https://example.org/c".to_owned(), b"ccccc".to_vec()),
    ]));
    let folder = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        folder.path().join("a"),
        folder.path().join("b"),
        folder.path().join("c"),
    );
    let mut said = Vec::new();

    let result = fetcher.fetch_many(
        &[
            ("https://example.org/a", &a),
            ("https://example.org/b", &b),
            ("https://example.org/c", &c),
        ],
        &mut |bytes| said.push(bytes),
        &|| false,
    );

    assert!(result.unwrap_err().contains("https://example.org/b"));
    assert!(!a.exists() && !b.exists() && !c.exists(), "none of them is left");
    assert_eq!(
        said,
        [3],
        "the bytes of the first were said, and the failing one was the second"
    );

    let mut said = Vec::new();
    fetcher
        .fetch_many(
            &[("https://example.org/a", &a), ("https://example.org/c", &c)],
            &mut |bytes| said.push(bytes),
            &|| false,
        )
        .unwrap();
    assert_eq!(said, [3, 8], "the bytes of the files so far, not of each");
    assert_eq!(
        (std::fs::read(&a).unwrap(), std::fs::read(&c).unwrap()),
        (b"aaa".to_vec(), b"ccccc".to_vec())
    );
}

// --- the notice ---

#[test]
fn the_notice_says_who_made_the_shaders_under_which_licence_where_the_source_is_and_that_they_are_unaltered() {
    let fixture = a_shader_fixture();
    let notice = notice_of_a_shader_set(&fixture.manifest.sets[0]);
    assert!(
        notice
            .starts_with("PACK abc1234\nBy Someone.\nLicence: MIT AND BSD-3-Clause\nHome page: https://example.org/\n"),
        "{notice}"
    );
    assert!(notice.contains("unaltered"));
    assert!(notice.contains("each file says whose it is and under which licence in its own header"));
    assert!(notice.contains("The source of this version is at: https://example.org/pack/tree/abc1234"));
    assert!(notice.ends_with("a note about the source\n"));

    let mut with_a_note = fixture.manifest.sets[0].clone();
    with_a_note.licence_note = Some("the licence of the repository".to_owned());
    assert!(
        notice_of_a_shader_set(&with_a_note)
            .contains("Licence: MIT AND BSD-3-Clause (the licence of the repository)\n")
    );
}

// --- curl: one run for many files ---

fn curl_is_there() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Serves each of `routes` (a path and its bytes) with a 404 for the rest, to as many as ask, from a thread, and a path that ends in `/slow` that never finishes. Gives the address.
fn a_server_with(routes: Vec<(&'static str, &'static [u8])>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            let routes = routes.clone();
            std::thread::spawn(move || {
                let mut request = [0_u8; 4096];
                let count = stream.read(&mut request).unwrap_or(0);
                let first_line = String::from_utf8_lossy(&request[..count])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned();
                let path = first_line.split_whitespace().nth(1).unwrap_or("").to_owned();
                if path.ends_with("/slow") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_secs(30));
                } else if let Some((_, body)) = routes.iter().find(|(route, _)| *route == path) {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(body);
                } else {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
            });
        }
    });
    format!("http://{address}")
}

#[test]
fn one_run_of_curl_fetches_every_file_and_says_how_many_bytes_there_are_of_all_of_them() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_with(vec![
        ("/a.fs", b"first shader"),
        ("/b%20c.fs", b"second"),
        ("/c.fs", b"third one"),
    ]);
    let folder = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        folder.path().join("a"),
        folder.path().join("with a space"),
        folder.path().join("c"),
    );
    let mut last = 0;

    Curl::for_a_test_on_this_computer()
        .fetch_many(
            &[
                (&format!("{server}/a.fs"), &a),
                (&format!("{server}/b%20c.fs"), &b),
                (&format!("{server}/c.fs"), &c),
            ],
            &mut |bytes| last = bytes,
            &|| false,
        )
        .unwrap();

    assert_eq!(std::fs::read(&a).unwrap(), b"first shader");
    assert_eq!(std::fs::read(&b).unwrap(), b"second");
    assert_eq!(std::fs::read(&c).unwrap(), b"third one");
    assert_eq!(last, 12 + 6 + 9);
    let left: Vec<String> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        left.len(),
        3,
        "the file that tells curl what to fetch is not left: {left:?}"
    );
}

#[test]
fn when_one_of_the_files_of_a_run_of_curl_is_missing_the_error_names_it_and_none_of_the_files_is_left() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_with(vec![("/a.fs", b"first"), ("/c.fs", b"third")]);
    let folder = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        folder.path().join("a"),
        folder.path().join("b"),
        folder.path().join("c"),
    );

    let error = Curl::for_a_test_on_this_computer()
        .fetch_many(
            &[
                (&format!("{server}/a.fs"), &a),
                (&format!("{server}/missing.fs"), &b),
                (&format!("{server}/c.fs"), &c),
            ],
            &mut |_| {},
            &|| false,
        )
        .unwrap_err();

    assert!(
        error.contains("/missing.fs") && error.contains("could not be downloaded"),
        "{error}"
    );
    assert!(!a.exists() && !b.exists() && !c.exists(), "none is left");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn a_run_of_curl_that_is_cancelled_is_stopped_at_once_and_leaves_no_file() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_with(vec![("/a.fs", b"first")]);
    let folder = tempfile::tempdir().unwrap();
    let (a, b) = (folder.path().join("a"), folder.path().join("b"));
    let began = Instant::now();
    let polls = AtomicUsize::new(0);

    let error = Curl::for_a_test_on_this_computer()
        .fetch_many(
            &[(&format!("{server}/a.fs"), &a), (&format!("{server}/slow"), &b)],
            &mut |_| {},
            &|| polls.fetch_add(1, Ordering::SeqCst) >= 3,
        )
        .unwrap_err();

    assert_eq!(error, CANCELLED);
    assert!(began.elapsed() < Duration::from_secs(10), "{:?}", began.elapsed());
    assert!(!a.exists() && !b.exists());
}

#[test]
fn a_run_of_curl_that_is_given_an_address_that_is_not_https_refuses_it() {
    if !curl_is_there() {
        eprintln!("SKIPPED: there is no curl");
        return;
    }
    let server = a_server_with(vec![("/a.fs", b"never given")]);
    let folder = tempfile::tempdir().unwrap();
    let a = folder.path().join("a");

    let error = Curl::https_only()
        .fetch_many(&[(&format!("{server}/a.fs"), &a)], &mut |_| {}, &|| false)
        .unwrap_err();

    assert!(error.contains("could not be downloaded"), "{error}");
    assert!(!a.exists());
}

#[test]
fn no_files_to_fetch_is_nothing_to_do() {
    Curl::https_only().fetch_many(&[], &mut |_| {}, &|| false).unwrap();
}

#[test]
fn the_file_that_tells_curl_what_to_fetch_quotes_every_address_and_every_path_and_escapes_what_must_be() {
    let to = Path::new("C:\\folder with \"quotes\"\\file");
    let text = crate::plugin_bundles::download::configuration_of_curl_for(&[("https://example.org/a b.fs", to)]);
    assert_eq!(
        text,
        "url = \"https://example.org/a b.fs\"\noutput = \"C:\\\\folder with \\\"quotes\\\"\\\\file\"\n"
    );
}

#[test]
fn a_failure_of_curl_to_check_revocation_is_tried_again_once_with_the_best_effort_flag_and_no_other_failure_is() {
    use crate::plugin_bundles::download::{is_a_failure_to_check_revocation, with_a_second_try_for_revocation};
    let offline = "curl: (35) schannel: next InitializeSecurityContext failed: CRYPT_E_REVOCATION_OFFLINE (0x80092013) - The revocation function was unable to check revocation because the revocation server was offline.";
    assert!(is_a_failure_to_check_revocation(offline));
    assert!(!is_a_failure_to_check_revocation(
        "curl: (22) The requested URL returned error: 404"
    ));

    let mut calls = Vec::new();
    let result = with_a_second_try_for_revocation(|best_effort| {
        calls.push(best_effort);
        if best_effort { Ok(()) } else { Err(offline.to_owned()) }
    });
    assert_eq!((result, calls), (Ok(()), vec![false, true]));

    let mut calls = Vec::new();
    let result = with_a_second_try_for_revocation(|best_effort| {
        calls.push(best_effort);
        Ok(())
    });
    assert_eq!((result, calls), (Ok(()), vec![false]), "no second try when it worked");

    let mut calls = Vec::new();
    let result = with_a_second_try_for_revocation(|best_effort| {
        calls.push(best_effort);
        Err("curl: (22) 404".to_owned())
    });
    assert_eq!(
        (result, calls),
        (Err("curl: (22) 404".to_owned()), vec![false]),
        "no second try for another failure"
    );

    let mut calls = Vec::new();
    let result = with_a_second_try_for_revocation(|best_effort| {
        calls.push(best_effort);
        Err(offline.to_owned())
    });
    assert_eq!(
        (result, calls),
        (Err(offline.to_owned()), vec![false, true]),
        "the second failure is what is said"
    );
}

#[test]
fn a_shader_that_says_something_has_other_bytes_than_one_that_says_another() {
    assert_ne!(a_shader_that_says("a"), a_shader_that_says("b"));
    assert_ne!(sha(&a_shader_that_says("a")), sha(&a_shader_that_says("b")));
}

// --- the real network (by hand) ---

/// **Fetches the shaders of the manifest from the network, with the system's `curl`**, into a folder of its own (about 0.4 MB; ignored unless asked for:
/// `cargo test --lib fetch_the_real_shaders -- --ignored --nocapture`), and then **looks for them as the plugin manager does**: every shader of the manifest must be found, must be
/// the kind of shader the manifest says, and must be one that this build can use. (Every checksum is checked by the download itself.) This is what keeps a shader that does not run out
/// of the manifest (B7 of `docs/engine-phase-6-6-design.md`).
#[test]
#[ignore = "downloads about 0.4 MB from the network"]
fn fetch_the_real_shaders_from_the_network_and_find_every_one_of_them_usable_and_of_the_kind_the_manifest_says() {
    let manifest = ShaderManifest::embedded();
    let folder = tempfile::tempdir().unwrap();
    let began = Instant::now();
    let outcome = fetch_shaders(&manifest, folder.path(), &Curl::https_only(), &mut |_| {}, &|| false).unwrap();
    println!("fetched {} shaders in {:?}", outcome.files, began.elapsed());

    let scan = crate::shader_files::scan_folders(&[folder.path().to_owned()]);

    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    for set in &manifest.sets {
        assert!(shaders_are_fetched(set, folder.path()), "{}", set.id);
        for shader in &set.shaders {
            let found = scan
                .shaders
                .iter()
                .find(|found| found.file_name == shader.file)
                .unwrap_or_else(|| panic!("{} was not found", shader.file));
            assert!(found.can_be_used(), "{}: {:?}", shader.file, found.why_not_usable);
            let shape = match found.shape.unwrap() {
                crate::isf::Shape::Filter => "filter",
                crate::isf::Shape::Generator => "generator",
                crate::isf::Shape::Transition => "transition",
            };
            assert_eq!(shape, shader.shape, "{}", shader.file);
        }
    }
    assert_eq!(scan.unusable_count(), 0);
}
