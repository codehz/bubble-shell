use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioReport {
    pub name: String,
    pub ok: bool,
    pub elapsed_ms: u128,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuiteReport {
    pub suite: String,
    pub ok: bool,
    pub scenarios: Vec<ScenarioReport>,
}

/// Records assertions without aborting the scenario. The first failure still
/// fails the scenario.
pub struct Probe {
    checks: Vec<Check>,
    failed: bool,
}

impl Probe {
    pub fn new() -> Self {
        Self { checks: Vec::new(), failed: false }
    }

    pub fn check(&mut self, name: impl Into<String>, ok: bool, detail: impl Into<String>) {
        if !ok {
            self.failed = true;
        }
        self.checks.push(Check { name: name.into(), ok, detail: detail.into() });
    }

    pub fn eq<T: PartialEq + std::fmt::Debug>(&mut self, name: &str, got: T, want: T) {
        let ok = got == want;
        let detail = if ok { format!("{got:?}") } else { format!("got {got:?}, want {want:?}") };
        self.check(name, ok, detail);
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn into_checks(self) -> Vec<Check> {
        self.checks
    }
}
