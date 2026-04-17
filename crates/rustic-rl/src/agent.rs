//! The interface the game loop talks to. `RLAgent` holds a policy and
//! decides an action per tick. Concrete policies live behind the `Policy`
//! trait so we can swap `Random`, `ScriptedHeuristic`, or a real model
//! without touching the game.

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::network::{greedy_action, Network, NullNetwork};
use crate::observe::{Action, Observation};

/// Config loaded from a named profile (e.g. `--rl-config=smol`).
/// Right now it only picks a policy flavor — real configs will include
/// model path, action thresholds, sample temperature, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub policy: PolicyKind,
    /// Threshold used by greedy network policies.
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_threshold() -> f32 {
    0.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyKind {
    /// Never presses anything — useful for wiring smoke tests.
    Idle,
    /// Presses uniformly at random. Stress-tests input plumbing.
    Random,
    /// Runs a `Network` and greedy-thresholds its output. Used once a real
    /// model is plugged in.
    Network,
}

impl Config {
    /// Load a named profile. For now only "smol" is recognized; anything
    /// else falls back to idle with a warning.
    pub fn load(name: &str) -> Self {
        match name {
            "smol" => Self {
                policy: PolicyKind::Random,
                threshold: default_threshold(),
            },
            other => {
                log::warn!("rustic-rl: unknown config '{other}', falling back to idle");
                Self {
                    policy: PolicyKind::Idle,
                    threshold: default_threshold(),
                }
            }
        }
    }
}

/// Policy lives on the game thread, so no Send/Sync bound — `ThreadRng`
/// (used by `RandomPolicy`) isn't Send anyway.
pub trait Policy {
    fn decide(&mut self, obs: &Observation) -> Action;
}

pub struct IdlePolicy;
impl Policy for IdlePolicy {
    fn decide(&mut self, _obs: &Observation) -> Action {
        Action::default()
    }
}

pub struct RandomPolicy {
    rng: rand::rngs::ThreadRng,
    /// Probability of pressing each lane on any given tick.
    pub press_prob: f32,
}

impl Default for RandomPolicy {
    fn default() -> Self {
        Self {
            rng: rand::rng(),
            press_prob: 0.05,
        }
    }
}

impl Policy for RandomPolicy {
    fn decide(&mut self, _obs: &Observation) -> Action {
        let mut press = [false; 4];
        for lane in press.iter_mut() {
            *lane = self.rng.random::<f32>() < self.press_prob;
        }
        Action { press }
    }
}

pub struct NetworkPolicy {
    pub net: Box<dyn Network>,
    pub threshold: f32,
}

impl Policy for NetworkPolicy {
    fn decide(&mut self, obs: &Observation) -> Action {
        let scores = self.net.forward(obs);
        greedy_action(scores, self.threshold)
    }
}

pub struct RLAgent {
    policy: Box<dyn Policy>,
    pub config: Config,
}

impl RLAgent {
    pub fn new(config: Config) -> Self {
        let policy: Box<dyn Policy> = match config.policy {
            PolicyKind::Idle => Box::new(IdlePolicy),
            PolicyKind::Random => Box::<RandomPolicy>::default(),
            PolicyKind::Network => Box::new(NetworkPolicy {
                net: Box::new(NullNetwork),
                threshold: config.threshold,
            }),
        };
        Self { policy, config }
    }

    pub fn decide(&mut self, obs: &Observation) -> Action {
        self.policy.decide(obs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_policy_presses_nothing() {
        let mut agent = RLAgent::new(Config {
            policy: PolicyKind::Idle,
            threshold: 0.5,
        });
        let act = agent.decide(&Observation::zero());
        assert_eq!(act.press, [false; 4]);
    }

    #[test]
    fn smol_config_uses_random_policy() {
        let cfg = Config::load("smol");
        assert_eq!(cfg.policy, PolicyKind::Random);
    }

    #[test]
    fn unknown_config_falls_back_to_idle() {
        let cfg = Config::load("does-not-exist");
        assert_eq!(cfg.policy, PolicyKind::Idle);
    }
}
