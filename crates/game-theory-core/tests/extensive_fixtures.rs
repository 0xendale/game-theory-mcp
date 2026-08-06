//! Loads extensive-form fixtures and checks that backward induction reproduces
//! the recorded subgame-perfect equilibria. See `fixtures/README.md` for the
//! schema; dropping a new `.json` into `fixtures/extensive/` adds a case.

use game_theory_core::{
    solve_backward_induction, verify_spe, ExtensiveGame, Rational, ValidExtensiveGame,
};
use num_traits::FromPrimitive;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct ExtensiveFixture {
    name: String,
    game: ExtensiveGame,
    expected_spe: ExpectedSpe,
}

#[derive(Deserialize)]
struct ExpectedSpe {
    paths: Vec<Vec<[usize; 2]>>,
    payoffs: Vec<Vec<f64>>,
}

type SpeKey = (Vec<(usize, usize)>, Vec<Rational>);

fn load(path: &Path) -> ExtensiveFixture {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn check(fixture: &ExtensiveFixture) {
    let game = ValidExtensiveGame::validate(fixture.game.clone())
        .unwrap_or_else(|e| panic!("{} is not a valid game: {e}", fixture.name));
    let result = solve_backward_induction(&game)
        .unwrap_or_else(|e| panic!("{} did not solve: {e}", fixture.name));

    assert_eq!(
        fixture.expected_spe.paths.len(),
        fixture.expected_spe.payoffs.len(),
        "{}: paths and payoffs must be parallel lists",
        fixture.name
    );

    let got: BTreeSet<SpeKey> = result
        .solutions
        .iter()
        .map(|s| (s.path.clone(), s.terminal_payoffs.clone()))
        .collect();
    let want: BTreeSet<SpeKey> = fixture
        .expected_spe
        .paths
        .iter()
        .zip(&fixture.expected_spe.payoffs)
        .map(|(path, payoff)| {
            (
                path.iter().map(|step| (step[0], step[1])).collect(),
                payoff
                    .iter()
                    .map(|&u| Rational::from_f64(u).expect("fixture payoffs are finite"))
                    .collect(),
            )
        })
        .collect();

    assert_eq!(got, want, "fixture {} SPE mismatch", fixture.name);

    // Independent of the solver: every solution it produced must also satisfy
    // the definition, as checked by verify_spe.
    for spe in &result.solutions {
        let checked = verify_spe(&game, &spe.profile)
            .unwrap_or_else(|e| panic!("{} failed verification: {e}", fixture.name));
        assert!(
            checked.holds,
            "{}: verify_spe rejected a solution the solver returned: {:?}",
            fixture.name, checked.deviation
        );
    }
}

fn fixture_paths() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensive");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn all_extensive_fixtures_reproduce_their_recorded_spe() {
    let paths = fixture_paths();
    assert!(
        paths.len() >= 2,
        "expected at least the two shipped fixtures, found {}",
        paths.len()
    );
    for path in paths {
        check(&load(&path));
    }
}
