# umasim library-test compilation diagnostic

Exit status: 101

```text
[1m[92m   Compiling[0m colored v3.1.1
[1m[92m   Compiling[0m lexopt v0.3.2
[1m[92m   Compiling[0m umasim v0.2.3 (/home/runner/work/umaai-rs/umaai-rs/crates/umasim)
[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/base/basic.rs:602:37
    [1m[94m|[0m
[1m[94m602[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/base/mod.rs:315:37
    [1m[94m|[0m
[1m[94m315[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/action.rs:1061:37
     [1m[94m|[0m
[1m[94m1061[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
     [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/effects.rs:375:37
    [1m[94m|[0m
[1m[94m375[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:1938:37
     [1m[94m|[0m
[1m[94m1938[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
     [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/policy.rs:764:37
    [1m[94m|[0m
[1m[94m764[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/rng_consistency.rs:26:33
    [1m[94m|[0m
[1m[94m 26[0m [1m[94m|[0m     utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                 [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:576:37
    [1m[94m|[0m
[1m[94m576[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/support_card.rs:338:37
    [1m[94m|[0m
[1m[94m338[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/uma.rs:305:37
    [1m[94m|[0m
[1m[94m305[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/game/mod.rs:180:37
    [1m[94m|[0m
[1m[94m180[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/output/turn_flow.rs:314:37
    [1m[94m|[0m
[1m[94m314[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/sampler.rs:709:37
    [1m[94m|[0m
[1m[94m709[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/search/flat_search.rs:795:37
    [1m[94m|[0m
[1m[94m795[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/trainer/logging_trainer.rs:162:37
    [1m[94m|[0m
[1m[94m162[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
   [1m[94m--> [0mcrates/umasim/src/trainer/ramen_handwritten_trainer.rs:165:37
    [1m[94m|[0m
[1m[94m165[0m [1m[94m|[0m         utils::{get_workspace_root, init_test_logger},
    [1m[94m|[0m                                     [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
    [1m[94m|[0m
--
[1m[94m124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
    [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0432][0m[1m: unresolved import `crate::utils::init_test_logger`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/action.rs:1415:41
     [1m[94m|[0m
[1m[94m1415[0m [1m[94m|[0m             utils::{get_workspace_root, init_test_logger},
     [1m[94m|[0m                                         [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mno `init_test_logger` in `utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0433][0m[1m: cannot find module or crate `flexi_logger` in this scope[0m
   [1m[94m--> [0mcrates/umasim/src/output/turn_flow.rs:413:24
    [1m[94m|[0m
[1m[94m413[0m [1m[94m|[0m             let spec = flexi_logger::LogSpecification::try_from("info")?;
    [1m[94m|[0m                        [1m[91m^^^^^^^^^^^^[0m [1m[91muse of unresolved module or unlinked crate `flexi_logger`[0m
    [1m[94m|[0m
    [1m[94m= [0m[1mhelp[0m: if you wanted to use a crate named `flexi_logger`, use `cargo add flexi_logger` to add it to your `Cargo.toml`

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1094:31
     [1m[94m|[0m
[1m[94m1094[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1117:31
     [1m[94m|[0m
[1m[94m1117[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1137:31
     [1m[94m|[0m
[1m[94m1137[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1151:31
     [1m[94m|[0m
[1m[94m1151[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1165:31
     [1m[94m|[0m
[1m[94m1165[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find function `init_test_logger` in module `crate::utils`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:1184:31
     [1m[94m|[0m
[1m[94m1184[0m [1m[94m|[0m         let _ = crate::utils::init_test_logger("info");
     [1m[94m|[0m                               [1m[91m^^^^^^^^^^^^^^^^[0m [1m[91mnot found in `crate::utils`[0m
     [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
    [1m[94m--> [0mcrates/umasim/src/utils.rs:124:8
     [1m[94m|[0m
--
[1m[94m 124[0m [1m[94m|[0m pub fn init_test_logger(spec: &str) -> Result<()> {
     [1m[94m|[0m        [1m[92m^^^^^^^^^^^^^^^^[0m

[1m[91merror[E0425][0m[1m: cannot find value `LOGGER` in module `crate::gamedata`[0m
   [1m[94m--> [0mcrates/umasim/src/output/turn_flow.rs:411:48
    [1m[94m|[0m
[1m[94m411[0m [1m[94m|[0m         if let Some(logger) = crate::gamedata::LOGGER.get() {
    [1m[94m|[0m                                                [1m[91m^^^^^^[0m [1m[91mnot found in `crate::gamedata`[0m
    [1m[94m|[0m
[1m[92mnote[0m: found an item that was configured out
   [1m[94m--> [0mcrates/umasim/src/gamedata/mod.rs:144:12
    [1m[94m|[0m
```

## Tail
```text
[1m[94m158[0m [1m[94m|[0m     use rand::SeedableRng;
    [1m[94m|[0m         [1m[33m^^^^^^^^^^^^^^^^^[0m

[1m[33mwarning[0m[1m: unused variable: `bonus_count`[0m
   [1m[94m--> [0mcrates/umasim/src/game/onsen/game.rs:440:17
    [1m[94m|[0m
[1m[94m440[0m [1m[94m|[0m             let bonus_count = -total_bonus[5] / 3;
    [1m[94m|[0m                 [1m[33m^^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_bonus_count`[0m
    [1m[94m|[0m
    [1m[94m= [0m[1mnote[0m: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

[1m[33mwarning[0m[1m: unused variable: `old_level`[0m
   [1m[94m--> [0mcrates/umasim/src/game/onsen/game.rs:880:13
    [1m[94m|[0m
[1m[94m880[0m [1m[94m|[0m         let old_level = self.dig_level[dig_type];
    [1m[94m|[0m             [1m[33m^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_old_level`[0m

[1m[33mwarning[0m[1m: unused variable: `removed_id`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/action.rs:466:21
    [1m[94m|[0m
[1m[94m466[0m [1m[94m|[0m                 let removed_id = game.base.distribution[train].remove(npc_pos);
    [1m[94m|[0m                     [1m[33m^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_removed_id`[0m

[1m[33mwarning[0m[1m: unused variable: `pt_before_reset`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:177:21
    [1m[94m|[0m
[1m[94m177[0m [1m[94m|[0m                 let pt_before_reset = self.ramen.scenario_pt;
    [1m[94m|[0m                     [1m[33m^^^^^^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_pt_before_reset`[0m

[1m[33mwarning[0m[1m: unused variable: `headers`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:517:13
    [1m[94m|[0m
[1m[94m517[0m [1m[94m|[0m         let headers: Vec<String> = base_headers
    [1m[94m|[0m             [1m[33m^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_headers`[0m

[1m[33mwarning[0m[1m: unused variable: `used_special`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:770:17
    [1m[94m|[0m
[1m[94m770[0m [1m[94m|[0m             let used_special = super::rules::consume_for_ramen(&mut self.ramen, ramen_idx, &targets)?;
    [1m[94m|[0m                 [1m[33m^^^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_used_special`[0m

[1m[33mwarning[0m[1m: unused variable: `dist_info`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:801:19
    [1m[94m|[0m
[1m[94m801[0m [1m[94m|[0m         if let Ok(dist_info) = self.explain_distribution() {
    [1m[94m|[0m                   [1m[33m^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_dist_info`[0m

[1m[33mwarning[0m[1m: unused variable: `removed_id`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:939:25
    [1m[94m|[0m
[1m[94m939[0m [1m[94m|[0m                     let removed_id = self.base.distribution[train].remove(npc_pos);
    [1m[94m|[0m                         [1m[33m^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_removed_id`[0m

[1m[33mwarning[0m[1m: unused variable: `index`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:1348:18
     [1m[94m|[0m
[1m[94m1348[0m [1m[94m|[0m             for (index, choice) in event.choices.iter().enumerate() {
     [1m[94m|[0m                  [1m[33m^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_index`[0m

[1m[33mwarning[0m[1m: unused variable: `choice`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:1348:25
     [1m[94m|[0m
[1m[94m1348[0m [1m[94m|[0m             for (index, choice) in event.choices.iter().enumerate() {
     [1m[94m|[0m                         [1m[33m^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_choice`[0m

[1m[33mwarning[0m[1m: unused variable: `year`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:1374:13
     [1m[94m|[0m
[1m[94m1374[0m [1m[94m|[0m         let year = year_idx + 1;
     [1m[94m|[0m             [1m[33m^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_year`[0m

[1m[33mwarning[0m[1m: unused variable: `names`[0m
    [1m[94m--> [0mcrates/umasim/src/game/ramen/game.rs:1386:17
     [1m[94m|[0m
[1m[94m1386[0m [1m[94m|[0m             let names: Vec<&str> = combo
     [1m[94m|[0m                 [1m[33m^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_names`[0m

[1m[33mwarning[0m[1m: unused variable: `before_stock`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:225:9
    [1m[94m|[0m
[1m[94m225[0m [1m[94m|[0m     let before_stock = state.feeling_stock;
    [1m[94m|[0m         [1m[33m^^^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_before_stock`[0m

[1m[33mwarning[0m[1m: unused variable: `before_special`[0m
   [1m[94m--> [0mcrates/umasim/src/game/ramen/rules.rs:226:9
    [1m[94m|[0m
[1m[94m226[0m [1m[94m|[0m     let before_special = state.special_feeling;
    [1m[94m|[0m         [1m[33m^^^^^^^^^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_before_special`[0m

[1m[33mwarning[0m[1m: unused variable: `index`[0m
   [1m[94m--> [0mcrates/umasim/src/game/traits.rs:114:18
    [1m[94m|[0m
[1m[94m114[0m [1m[94m|[0m             for (index, choice) in event.choices.iter().enumerate() {
    [1m[94m|[0m                  [1m[33m^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_index`[0m

[1m[33mwarning[0m[1m: unused variable: `choice`[0m
   [1m[94m--> [0mcrates/umasim/src/game/traits.rs:114:25
    [1m[94m|[0m
[1m[94m114[0m [1m[94m|[0m             for (index, choice) in event.choices.iter().enumerate() {
    [1m[94m|[0m                         [1m[33m^^^^^^[0m [1m[33mhelp: if this is intentional, prefix it with an underscore: `_choice`[0m

[1m[33mwarning[0m[1m: value assigned to `guard` is never read[0m
   [1m[94m--> [0mcrates/umasim/src/trainer/local_ramen_trainer.rs:538:13
    [1m[94m|[0m
[1m[94m538[0m [1m[94m|[0m [1m[33m/[0m             guard = out
[1m[94m539[0m [1m[94m|[0m [1m[33m|[0m                 .iter()
[1m[94m540[0m [1m[94m|[0m [1m[33m|[0m                 .enumerate()
[1m[94m541[0m [1m[94m|[0m [1m[33m|[0m                 .filter(|(i, _)| a.get(*i).is_some_and(|x| matches!(x.operation, Operation::Train(_))))
[1m[94m542[0m [1m[94m|[0m [1m[33m|[0m                 .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
[1m[94m543[0m [1m[94m|[0m [1m[33m|[0m                 .map(|(i, _)| i)
[1m[94m544[0m [1m[94m|[0m [1m[33m|[0m                 .ok_or_else(|| anyhow::anyhow!("已吃面但 Train 阶段没有训练候选"))?;
    [1m[94m|[0m [1m[33m|___________________________________________________________________________________^[0m
    [1m[94m|[0m
    [1m[94m= [0m[1mhelp[0m: maybe it is overwritten before being read?
    [1m[94m= [0m[1mnote[0m: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

[1mSome errors have detailed explanations: E0425, E0432, E0433.[0m
[1mFor more information about an error, try `rustc --explain E0425`.[0m
[1m[33mwarning[0m: `umasim` (lib test) generated 23 warnings
[1m[91merror[0m: could not compile `umasim` (lib test) due to 25 previous errors; 23 warnings emitted
```
