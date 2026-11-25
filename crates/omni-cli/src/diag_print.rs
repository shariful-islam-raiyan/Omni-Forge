use omni_core::diagnostics::Diagnostic;

pub fn print_diagnostic(d: &Diagnostic) {
    eprintln!("{d}");
    if let Some(explanation) = &d.explanation {
        eprintln!("  {explanation}");
    }
}

pub fn print_all(diags: &[Diagnostic]) {
    for d in diags {
        print_diagnostic(d);
    }
}
