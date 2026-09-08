use std::path::PathBuf;

use xsc_core::r#static::info::{gen_errs_from_src, AstMap, TypeEnv, XsError};

fn errors(src: &str) -> Vec<XsError> {
    let path = PathBuf::from("variable_initializers.xs");
    let mut env = TypeEnv::new(vec![]);
    let mut ast_cache = AstMap::new();
    let src_cache = AstMap::new();
    gen_errs_from_src(&path, src, &mut env, &mut ast_cache, &src_cache)
        .expect("declarations should parse, including those with a missing initializer");
    env.errs()
        .get(&path)
        .into_iter()
        .flatten()
        .filter(|err| !err.is_warning())
        .cloned()
        .collect()
}

#[test]
fn every_variable_type_requires_an_initializer_in_both_scopes() {
    for type_name in ["int", "float", "bool", "string", "vector"] {
        for declaration in [
            format!("{type_name} pending;"),
            format!("void main() {{ {type_name} pending; }}"),
        ] {
            let errors = errors(&declaration);
            assert_eq!(errors.len(), 1, "{declaration}: {errors:?}");
            assert!(
                matches!(&errors[0], XsError::Syntax { msg, .. } if msg.contains("initialised"))
            );
            let span = errors[0].span();
            assert_eq!(&declaration[span.start..span.end], "pending");
        }
    }
}

#[test]
fn modifiers_do_not_make_an_initializer_optional() {
    for src in [
        "const int pending;",
        "extern int pending;",
        "export int pending;",
        "void main() { static int pending; }",
    ] {
        let errors = errors(src);
        assert_eq!(errors.len(), 1, "{src}: {errors:?}");
        assert!(matches!(&errors[0], XsError::Syntax { msg, .. } if msg.contains("initialised")));
    }
}

#[test]
fn assigning_later_does_not_replace_a_declaration_initializer() {
    let errors = errors("void main() { int pending; pending = 1; }");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(&errors[0], XsError::Syntax { msg, .. } if msg.contains("initialised")));
}

#[test]
fn literal_initializers_remain_valid() {
    for declaration in [
        "int pending = 0;",
        "float pending = 0.0;",
        "bool pending = false;",
        "string pending = \"\";",
        "vector pending = vector(0, 0, 0);",
    ] {
        for src in [
            declaration.to_string(),
            format!("void main() {{ {declaration} }}"),
        ] {
            assert!(errors(&src).is_empty(), "{src}");
        }
    }
}

#[test]
fn const_and_local_expression_initializers_remain_valid() {
    let src = "const int limit = 4; int count = limit;
        int next() { return (1); }
        void main() { int pending = next(); pending = pending + limit; }";
    assert!(errors(src).is_empty());
}
