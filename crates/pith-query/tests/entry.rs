use std::path::Path;

use pith_output::dto::{AboutValueRepr, EvaluationSourceRepr, TierRepr, ValueRepr};
use pith_query::{ReadOnly, Roots, Session, Writable, explore};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn module(directory: &Path, text: &str) -> TestResult<std::path::PathBuf> {
    let path = directory.join("root.pi");
    std::fs::write(&path, text)?;
    Ok(path)
}

fn writable(home: &Path) -> TestResult<Session<Writable>> {
    Ok(Session::open_writable(Roots::under(home))?)
}

#[test]
fn explore_includes_entries_and_about_metadata() -> TestResult {
    let source = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "-- module facts\nabout {\n  description: \"entry fixture\",\n  maintainers: [\"query\"],\n}\n\npure rule echo(value: Text) -> Text = { value }\n\n-- default request\nentry main : Text = ask (\"hello\")\n",
    )?;

    let explored = explore(&path)?;

    let [entry] = explored.entries.as_ref() else {
        return Err("explore omitted the entry".into());
    };
    assert_eq!(entry.coordinate.as_ref(), "root::entry.main");
    assert!(matches!(entry.tier, TierRepr::Represented));
    assert_eq!(entry.documentation.as_ref(), "default request");
    let [about] = explored.about.as_ref() else {
        return Err("explore omitted the about block".into());
    };
    let Some((_, description)) = about.fields.first() else {
        return Err("explore omitted the about fields".into());
    };
    assert!(matches!(
        description,
        AboutValueRepr::Text { text } if text.as_ref() == "entry fixture"
    ));
    assert_eq!(about.documentation.as_ref(), "module facts");
    Ok(())
}

#[test]
fn an_entry_evaluates_and_hydrates_by_its_body_digest() -> TestResult {
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "pure rule echo(value: Text) -> Text = { value }\n\nentry main : Text = ask (\"hello\")\n",
    )?;

    let first = writable(home.path())?.run_entry(&path, "main")?;
    let second = writable(home.path())?.run_entry(&path, "main")?;

    assert!(matches!(first.source, EvaluationSourceRepr::Computed));
    assert!(matches!(second.source, EvaluationSourceRepr::Hydrated));
    assert!(matches!(first.value, ValueRepr::Text { ref s } if s.as_ref() == "hello"));
    Ok(())
}

#[test]
fn entry_selection_is_read_only_and_names_the_synthetic_coordinate() -> TestResult {
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "pure rule echo(value: Text) -> Text = { value }\n\nentry main : Text = ask (\"hello\")\n",
    )?;

    let selected =
        Session::<ReadOnly>::open(Roots::under(home.path()))?.select_entry(&path, "main")?;

    assert_eq!(selected.rule.as_ref(), "root::entry.main");
    assert!(!home.path().join("state.db").exists());
    Ok(())
}

#[test]
fn a_colliding_entry_teaches_that_its_name_is_not_a_preference() -> TestResult {
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "pure rule answer() -> Text = { \"hello\" }\n\nentry main : Text = ask ()\n",
    )?;

    let error = writable(home.path())?
        .run_entry(&path, "main")
        .err()
        .ok_or("the collision evaluated")?;

    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.0 == 1102 && diagnostic.message.0.contains("not a preferred rule")
    }));
    Ok(())
}

#[test]
fn an_unbound_host_rule_names_the_coordinate() -> TestResult {
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "pure rule echo(Text) -> Text = host\n\nentry main : Text = ask (\"hello\")\n",
    )?;

    let error = writable(home.path())?
        .run_entry(&path, "main")
        .err()
        .ok_or("the unbound host rule evaluated")?;

    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.message.0.contains("root.echo")
            && diagnostic.message.0.contains("links no domain crate")
    }));
    Ok(())
}

#[test]
fn the_builtin_exec_type_is_evaluated_before_the_caller_effect() -> TestResult {
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let path = module(
        source.path(),
        "import pith\n\npure rule command(name: Text) -> pith.Exec = {\n  Exec({arguments: [name], program: \"/bin/echo\"})\n}\n\nentry dev : pith.Exec = ask (\"hello\")\n",
    )?;

    let invocation = writable(home.path())?.prepare_exec(&path, "dev")?;

    assert_eq!(invocation.program.as_ref(), "/bin/echo");
    assert_eq!(invocation.arguments.as_ref(), [Box::<str>::from("hello")]);
    Ok(())
}
