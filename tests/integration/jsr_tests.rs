// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use deno_core::serde_json::json;
use deno_core::serde_json::Value;
use deno_lockfile::Lockfile;
use test_util as util;
use test_util::itest;
use url::Url;
use util::assert_contains;
use util::assert_not_contains;
use util::env_vars_for_jsr_npm_tests;
use util::env_vars_for_jsr_tests;
use util::TestContextBuilder;

itest!(no_module_graph_run {
  args: "run jsr/no_module_graph/main.ts",
  output: "jsr/no_module_graph/main.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(no_module_graph_info {
  args: "info jsr/no_module_graph/main.ts",
  output: "jsr/no_module_graph/main_info.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(same_package_multiple_versions {
  args: "run --quiet jsr/no_module_graph/multiple.ts",
  output: "jsr/no_module_graph/multiple.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(module_graph_run {
  args: "run jsr/module_graph/main.ts",
  output: "jsr/module_graph/main.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(module_graph_info {
  args: "info jsr/module_graph/main.ts",
  output: "jsr/module_graph/main_info.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(deps_run {
  args: "run jsr/deps/main.ts",
  output: "jsr/deps/main.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(deps_info {
  args: "info jsr/deps/main.ts",
  output: "jsr/deps/main_info.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
});

itest!(import_https_url_analyzable {
  args: "run -A jsr/import_https_url/analyzable.ts",
  output: "jsr/import_https_url/analyzable.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
  exit_code: 1,
});

itest!(import_https_url_unanalyzable {
  args: "run -A jsr/import_https_url/unanalyzable.ts",
  output: "jsr/import_https_url/unanalyzable.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
  exit_code: 1,
});

itest!(subset_type_graph {
  args: "check --all jsr/subset_type_graph/main.ts",
  output: "jsr/subset_type_graph/main.check.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
  exit_code: 1,
});

#[test]
fn fast_check_cache() {
  let test_context = TestContextBuilder::for_jsr().use_temp_cwd().build();
  let deno_dir = test_context.deno_dir();
  let temp_dir = test_context.temp_dir();
  let type_check_cache_path = deno_dir.path().join("check_cache_v1");

  temp_dir.write(
    "main.ts",
    r#"import { add } from "jsr:@denotest/add@1";
    const value: number = add(1, 2);
    console.log(value);"#,
  );
  temp_dir.path().join("deno.json").write_json(&json!({
    "vendor": true
  }));

  test_context
    .new_command()
    .args("check main.ts")
    .run()
    .skip_output_check();

  type_check_cache_path.remove_file();
  let check_debug_cmd = test_context
    .new_command()
    .args("check --log-level=debug main.ts");
  let output = check_debug_cmd.run();
  assert_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );

  // modify the file in the vendor folder
  let vendor_dir = temp_dir.path().join("vendor");
  let pkg_dir = vendor_dir.join("http_127.0.0.1_4250/@denotest/add/1.0.0/");
  pkg_dir
    .join("mod.ts")
    .append("\nexport * from './other.ts';");
  let nested_pkg_file = pkg_dir.join("other.ts");
  nested_pkg_file.write("export function other(): string { return ''; }");

  // invalidated
  let output = check_debug_cmd.run();
  assert_not_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );

  // ensure cache works
  let output = check_debug_cmd.run();
  assert_contains!(output.combined_output(), "Already type checked.");
  let building_fast_check_msg = "Building fast check graph";
  assert_not_contains!(output.combined_output(), building_fast_check_msg);

  // now validated
  type_check_cache_path.remove_file();
  let output = check_debug_cmd.run();
  assert_contains!(output.combined_output(), building_fast_check_msg);
  assert_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );

  // cause a fast check error in the nested package
  nested_pkg_file
    .append("\nexport function asdf(a: number) { let err: number = ''; return Math.random(); }");
  check_debug_cmd.run().skip_output_check();

  // ensure the cache still picks it up for this file
  type_check_cache_path.remove_file();
  let output = check_debug_cmd.run();
  assert_contains!(output.combined_output(), building_fast_check_msg);
  assert_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );

  // see that the type checking error in the internal function gets surfaced with --all
  test_context
    .new_command()
    .args("check --all main.ts")
    .run()
    .assert_matches_text(
      "Check file:///[WILDCARD]main.ts
error: TS2322 [ERROR]: Type 'string' is not assignable to type 'number'.
export function asdf(a: number) { let err: number = ''; return Math.random(); }
                                      ~~~
    at http://127.0.0.1:4250/@denotest/add/1.0.0/other.ts:2:39
",
    )
    .assert_exit_code(1);

  // now fix the package
  nested_pkg_file.write("export function test() {}");
  let output = check_debug_cmd.run();
  assert_contains!(output.combined_output(), building_fast_check_msg);
  assert_not_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );

  // finally ensure it uses the cache
  type_check_cache_path.remove_file();
  let output = check_debug_cmd.run();
  assert_contains!(output.combined_output(), building_fast_check_msg);
  assert_contains!(
    output.combined_output(),
    "Using FastCheck cache for: @denotest/add@1.0.0"
  );
}

itest!(version_not_found {
  args: "run jsr/version_not_found/main.ts",
  output: "jsr/version_not_found/main.out",
  envs: env_vars_for_jsr_tests(),
  http_server: true,
  exit_code: 1,
});

#[test]
fn specifiers_in_lockfile() {
  let test_context = TestContextBuilder::for_jsr().use_temp_cwd().build();
  let temp_dir = test_context.temp_dir();

  temp_dir.write(
    "main.ts",
    r#"import version from "jsr:@denotest/no_module_graph@0.1";

console.log(version);"#,
  );
  temp_dir.write("deno.json", "{}"); // to automatically create a lockfile

  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text("0.1.1\n");

  let lockfile_path = temp_dir.path().join("deno.lock");
  let mut lockfile = Lockfile::new(lockfile_path.to_path_buf(), false).unwrap();
  *lockfile
    .content
    .packages
    .specifiers
    .get_mut("jsr:@denotest/no_module_graph@0.1")
    .unwrap() = "jsr:@denotest/no_module_graph@0.1.0".to_string();
  lockfile_path.write(lockfile.as_json_string());

  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text("0.1.0\n");
}

#[test]
fn reload_info_not_found_cache_but_exists_remote() {
  fn remove_version(registry_json: &mut Value, version: &str) {
    registry_json
      .as_object_mut()
      .unwrap()
      .get_mut("versions")
      .unwrap()
      .as_object_mut()
      .unwrap()
      .remove(version);
  }

  fn remove_version_for_package(
    deno_dir: &util::TempDir,
    package: &str,
    version: &str,
  ) {
    let specifier =
      Url::parse(&format!("http://127.0.0.1:4250/{}/meta.json", package))
        .unwrap();
    let registry_json_path = deno_dir
      .path()
      .join("deps")
      .join(deno_cache_dir::url_to_filename(&specifier).unwrap());
    let mut registry_json = registry_json_path.read_json_value();
    remove_version(&mut registry_json, version);
    registry_json_path.write_json(&registry_json);
  }

  // This tests that when a local machine doesn't have a version
  // specified in a dependency that exists in the npm registry
  let test_context = TestContextBuilder::for_jsr().use_temp_cwd().build();
  let deno_dir = test_context.deno_dir();
  let temp_dir = test_context.temp_dir();
  temp_dir.write(
    "main.ts",
    "import { add } from 'jsr:@denotest/add@1'; console.log(add(1, 2));",
  );

  // cache successfully to the deno_dir
  let output = test_context.new_command().args("cache main.ts").run();
  output.assert_matches_text(concat!(
    "Download http://127.0.0.1:4250/@denotest/add/meta.json\n",
    "Download http://127.0.0.1:4250/@denotest/add/1.0.0_meta.json\n",
    "Download http://127.0.0.1:4250/@denotest/add/1.0.0/mod.ts\n",
  ));

  // modify the package information in the cache to remove the latest version
  remove_version_for_package(deno_dir, "@denotest/add", "1.0.0");

  // should error when `--cache-only` is used now because the version is not in the cache
  let output = test_context
    .new_command()
    .args("run --cached-only main.ts")
    .run();
  output.assert_exit_code(1);
  output.assert_matches_text("error: Failed to resolve version constraint. Try running again without --cached-only
    at file:///[WILDCARD]main.ts:1:21
");

  // now try running without it, it should download the package now
  test_context
    .new_command()
    .args("run main.ts")
    .run()
    .assert_matches_text(concat!(
      "Download http://127.0.0.1:4250/@denotest/add/meta.json\n",
      "Download http://127.0.0.1:4250/@denotest/add/1.0.0_meta.json\n",
      "3\n",
    ))
    .assert_exit_code(0);
}

#[test]
fn lockfile_bad_package_integrity() {
  let test_context = TestContextBuilder::for_jsr().use_temp_cwd().build();
  let temp_dir = test_context.temp_dir();

  temp_dir.write(
    "main.ts",
    r#"import version from "jsr:@denotest/no_module_graph@0.1";

console.log(version);"#,
  );
  temp_dir.write("deno.json", "{}"); // to automatically create a lockfile

  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text("0.1.1\n");

  let lockfile_path = temp_dir.path().join("deno.lock");
  let mut lockfile = Lockfile::new(lockfile_path.to_path_buf(), false).unwrap();
  let pkg_name = "@denotest/no_module_graph@0.1.1";
  let original_integrity = get_lockfile_pkg_integrity(&lockfile, pkg_name);
  set_lockfile_pkg_integrity(&mut lockfile, pkg_name, "bad_integrity");
  lockfile_path.write(lockfile.as_json_string());

  let actual_integrity =
    test_context.get_jsr_package_integrity("@denotest/no_module_graph/0.1.1");
  let integrity_check_failed_msg = format!("error: Integrity check failed for http://127.0.0.1:4250/@denotest/no_module_graph/0.1.1_meta.json

Actual: {}
Expected: bad_integrity
    at file:///[WILDCARD]/main.ts:1:21
", actual_integrity);
  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text(&integrity_check_failed_msg)
    .assert_exit_code(1);

  // now try with a vendor folder
  temp_dir
    .path()
    .join("deno.json")
    .write_json(&json!({ "vendor": true }));

  // should fail again
  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text(&integrity_check_failed_msg)
    .assert_exit_code(1);

  // now update to the correct integrity
  set_lockfile_pkg_integrity(&mut lockfile, pkg_name, &original_integrity);
  lockfile_path.write(lockfile.as_json_string());

  // should pass now
  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text("0.1.1\n")
    .assert_exit_code(0);

  // now update to a bad integrity again
  set_lockfile_pkg_integrity(&mut lockfile, pkg_name, "bad_integrity");
  lockfile_path.write(lockfile.as_json_string());

  // shouldn't matter because we have a vendor folder
  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text("0.1.1\n")
    .assert_exit_code(0);

  // now remove the vendor dir and it should fail again
  temp_dir.path().join("vendor").remove_dir_all();

  test_context
    .new_command()
    .args("run --quiet main.ts")
    .run()
    .assert_matches_text(&integrity_check_failed_msg)
    .assert_exit_code(1);
}

#[test]
fn bad_manifest_checksum() {
  let test_context = TestContextBuilder::for_jsr().use_temp_cwd().build();
  let temp_dir = test_context.temp_dir();

  temp_dir.write(
    "main.ts",
    r#"import { add } from "jsr:@denotest/bad-manifest-checksum@1.0.0";
console.log(add);"#,
  );

  // test it properly checks the checksum on download
  test_context
    .new_command()
    .args("run  main.ts")
    .run()
    .assert_matches_text(
      "Download http://127.0.0.1:4250/@denotest/bad-manifest-checksum/meta.json
Download http://127.0.0.1:4250/@denotest/bad-manifest-checksum/1.0.0_meta.json
Download http://127.0.0.1:4250/@denotest/bad-manifest-checksum/1.0.0/mod.ts
error: Integrity check failed.

Actual: 9a30ac96b5d5c1b67eca69e1e2cf0798817d9578c8d7d904a81a67b983b35cba
Expected: bad-checksum
    at file:///[WILDCARD]main.ts:1:21
",
    )
    .assert_exit_code(1);

  // test it properly checks the checksum when loading from the cache
  test_context
    .new_command()
    .args("run  main.ts")
    .run()
    .assert_matches_text(
      // ideally the two error messages would be the same... this one comes from
      // deno_cache and the one above comes from deno_graph. The thing is, in deno_cache
      // (source of this error) it makes sense to include the url in the error message
      // because it's not always used in the context of deno_graph
      "error: Integrity check failed for http://127.0.0.1:4250/@denotest/bad-manifest-checksum/1.0.0/mod.ts

Actual: 9a30ac96b5d5c1b67eca69e1e2cf0798817d9578c8d7d904a81a67b983b35cba
Expected: bad-checksum
    at file:///[WILDCARD]main.ts:1:21
",
    )
    .assert_exit_code(1);
}

fn get_lockfile_pkg_integrity(lockfile: &Lockfile, pkg_name: &str) -> String {
  lockfile
    .content
    .packages
    .jsr
    .get(pkg_name)
    .unwrap()
    .integrity
    .clone()
}

fn set_lockfile_pkg_integrity(
  lockfile: &mut Lockfile,
  pkg_name: &str,
  integrity: &str,
) {
  lockfile
    .content
    .packages
    .jsr
    .get_mut(pkg_name)
    .unwrap()
    .integrity = integrity.to_string();
}

itest!(jsx_with_no_pragmas {
  args: "run jsr/jsx_with_no_pragmas/main.ts",
  output: "jsr/jsx_with_no_pragmas/main.out",
  envs: env_vars_for_jsr_npm_tests(),
  http_server: true,
  exit_code: 1,
});

itest!(jsx_with_pragmas {
  args: "run jsr/jsx_with_pragmas/main.ts",
  output: "jsr/jsx_with_pragmas/main.out",
  envs: env_vars_for_jsr_npm_tests(),
  http_server: true,
  exit_code: 0,
});
