from pathlib import Path

path = Path("crates/umasim/src/search/flat_search.rs")
text = path.read_text()

old_import = "onsen::{OnsenTurnStage, action::OnsenAction, game::OnsenGame},"
new_import = "onsen::{action::OnsenAction, game::OnsenGame},"
assert text.count(old_import) == 1
text = text.replace(old_import, new_import)

old_debug = '''            "[回合 {}] 开始搜索: {} 个动作, search_n={}, max_depth={}, radical_factor={:.1}, ucb={}, 根种子={:#018x}",
            game.turn(),
            actions.len(),
            self.config.search_n,
            self.config.max_depth,
            radical_factor,'''
new_debug = '''            "[回合 {}] 开始搜索: {} 个动作, search_n={}, max_depth={}, leaf_eval={}, radical_factor={:.1}, ucb={}, 根种子={:#018x}",
            game.turn(),
            actions.len(),
            self.config.search_n,
            self.config.max_depth,
            self.leaf_evaluator.name(),
            radical_factor,'''
assert text.count(old_debug) == 1
text = text.replace(old_debug, new_debug)

obsolete = '''/// 回合阶段编号（种子派生用）
///
/// 显式 match 而非依赖枚举判别值：`OnsenTurnStage` 的变体顺序若调整，
/// 这里会编译报错提醒同步，而不是静默改变所有历史种子。
fn stage_id(stage: &OnsenTurnStage) -> u64 {
    match stage {
        OnsenTurnStage::Begin => 0,
        OnsenTurnStage::Distribute => 1,
        OnsenTurnStage::Bathing => 2,
        OnsenTurnStage::Train => 3,
        OnsenTurnStage::AfterTrain => 4
    }
}

'''
assert text.count(obsolete) == 1
text = text.replace(obsolete, "")
path.write_text(text)
