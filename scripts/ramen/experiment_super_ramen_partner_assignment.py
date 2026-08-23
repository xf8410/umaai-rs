#!/usr/bin/env python3
from pathlib import Path

path = Path("crates/umasim/src/game/ramen/action.rs")
text = path.read_text()
old = '''        // 获取所有支援卡索引（含友人卡，index 0-5）
        let card_indices: Vec<i32> = (0..6i32)
            .filter(|&i| {
                let person = &game.persons[i as usize];
                person.person_type == PersonType::Card || person.person_type == PersonType::ScenarioCard
            })
            .collect();

        if card_indices.is_empty() {
            return Ok(());
        }

        // 对每个支援卡，随机分配到一个训练位置，失败则重试
        for &person_idx in &card_indices {
            let mut success = false;
            let max_retries = option_trains.len() * 2; // 最多重试次数

            for _ in 0..max_retries {
                // 随机选择一个训练位置
                let &train = option_trains.choose(rng).unwrap();
                let train = train as usize;

                match Self::try_add_clone(game, person_idx, train) {
                    Ok(()) => {
                        success = true;
                        break;
                    }
                    Err(_) => continue // 分配失败，重试
                }
            }

            if !success {
                diag!(
                    ">> 超级拉面分身失败: {} 无法分配到任何训练位置",
                    game.persons[person_idx as usize].short_name()
                );
            }
        }
'''
new = '''        // 动态获取当前人物表中的全部携带支援卡人头，包括后续加入的友人卡。
        // 不依赖 persons 与 deck 的固定索引布局。
        let card_indices: Vec<i32> = game
            .persons
            .iter()
            .enumerate()
            .filter_map(|(index, person)| {
                matches!(person.person_type, PersonType::Card | PersonType::ScenarioCard)
                    .then_some(index as i32)
            })
            .collect();

        if card_indices.is_empty() {
            return Ok(());
        }

        // 正常人头已按普通得意率规则分配。这里只决定超级拉面的额外人头位置：
        // 排除同一人物已在的位置和无法容纳额外非 NPC 人头的位置，再从合法位置随机选择。
        for &person_idx in &card_indices {
            let available_trains: Vec<usize> = option_trains
                .iter()
                .copied()
                .filter_map(|train| usize::try_from(train).ok())
                .filter(|&train| {
                    if train >= 5 || game.base.distribution[train].contains(&person_idx) {
                        return false;
                    }
                    let dist = &game.base.distribution[train];
                    let non_npc_count = dist
                        .iter()
                        .filter(|&&id| id >= 0 && game.persons[id as usize].person_type != PersonType::Npc)
                        .count();
                    non_npc_count < 5
                        && (dist.len() < 5
                            || dist.iter().any(|&id| {
                                id >= 0 && game.persons[id as usize].person_type == PersonType::Npc
                            }))
                })
                .collect();

            if let Some(&train) = available_trains.choose(rng) {
                Self::try_add_clone(game, person_idx, train)?;
            } else {
                diag!(
                    ">> 超级拉面分身失败: {} 无合法训练位置",
                    game.persons[person_idx as usize].short_name()
                );
            }
        }
'''
if text.count(old) != 1:
    raise SystemExit(f"expected exactly one legacy block, found {text.count(old)}")
path.write_text(text.replace(old, new))
