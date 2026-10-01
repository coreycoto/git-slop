use super::super::has_inline_tests;

#[test]
fn javascript_inline_tests_ignore_methods_identifiers_and_literal_text() {
    for source in [
        "export const valid = (value) => /^ok$/.test(value);",
        "export const parts = (value) => value.split(',');",
        "emit('value', callback); submit('value', callback); $it('value', callback);",
        "étest('value', callback); testCase('value', callback);",
        "client.test('value', callback); client?.test('value', callback);",
        "client /* formatting */ . test ('value', callback);",
        "// test('fake', () => {});\nexport const value = 1;",
        "/* describe('fake', () => { it('fake', callback); }); */",
        r#"const message = "test('fake', () => {})";"#,
        r#"const message = 'it("fake", callback)';"#,
        "const message = `describe('fake', () => {})`;",
        "const message = `${`test('fake', callback)`} it('fake', callback)`;",
        r#"const pattern = /test\('fake', callback\)/;"#,
        "export function test(value, callback) { return callback(value); }",
        "class Validator { test(value, callback) { return callback(value); } }",
        "const object = { it(value, callback) {} };",
        "test('no callback'); it(); describe;",
    ] {
        for language in ["JavaScript", "JSX", "TypeScript", "TSX"] {
            assert!(!has_inline_tests(language, source), "{language}: {source}");
        }
    }
}

#[test]
fn javascript_inline_tests_recognize_definitions_across_formatting() {
    for source in [
        "test('works', () => {});",
        "it(\"works\", function () {});",
        "describe('suite', () => { it('works', () => {}); });",
        "test \n (\n 'works',\n async () => {}\n);",
        "it /* label */ ( `works`, /* body */ () => {} );",
        "describe\r\n('suite', function () {});",
        "test.only('works', () => {}); it.skip('works', callback);",
        "test(name, callback);",
        "const quotient = value / divisor; test('works', () => {});",
        "// it('fake', callback)\n test('real', () => {});",
        "const message = `it('fake', callback)`; test('real', callback);",
    ] {
        for language in ["JavaScript", "JSX", "TypeScript", "TSX"] {
            assert!(has_inline_tests(language, source), "{language}: {source}");
        }
    }
}
