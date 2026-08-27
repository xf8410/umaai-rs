# 测试一览

本文件按模块分类、用一句话描述每个测试的功能，便于快速定位和评估覆盖率。

总计：**318 个测试**（含 lib 312 passed + 3 baseline 失败 + 3 ignored），分布在 `crates/umasim/src/` 各文件中。

## 目录

- [拉面杯](#拉面杯) — 157 个
  - [`game.rs`](#gamers阶段流转与集成) — 48
  - [`rules.rs`](#rulesrs诀窍吃面与地区) — 30
  - [`policy.rs`](#policyrs固定策略与手写策略核心) — 24
  - [`action.rs`](#actionrs动作枚举与列表) — 21
  - [`effects.rs`](#effectsrs训练数值与得意率) — 15
  - [`features.rs`](#featuresrs拉面局面特征编码器) — 9
  - [`events.rs`](#eventsrs友人事件与剧本事件) — 5
  - [`rng_consistency.rs`](#rng_consistencyrsrng-受控重构集成测试) — 4
  - [`state.rs`](#staters剧本状态) — 1
- [基础游戏](#基础游戏) — 14
  - [`game/base/mod.rs`](#gamebasemodrs基础游戏与五维上限) — 12
  - [`game/base/basic.rs`](#gamebasebasicrsbasic-剧本) — 2
- [搜索](#搜索) — 34 个
  - [`search/flat_search.rs`](#searchflat_searchrs扁平蒙特卡洛搜索) — 20
  - [`search/seeds.rs`](#searchseedsrsrollout-种子派生) — 6
  - [`search/ramen_terminal.rs`](#searchramen_terminalrs拉面终局观测) — 5
  - [`search/terminal.rs`](#searchterminalrs通用终局统计) — 2
  - [`search/config.rs`](#searchconfigrs搜索配置契约) — 1
- [训练员](#训练员) — 31 个
  - [`trainer/ramen_mcts_trainer.rs`](#trainerramen_mcts_trainerrs拉面-mcts-训练员) — 15
  - [`trainer/local_ramen_trainer.rs`](#trainerlocal_ramen_trainerrs本地手写策略) — 12
  - [`trainer/logging_trainer.rs`](#trainerlogging_trainerrs决策日志包装) — 2
  - [`trainer/ramen_handwritten_trainer.rs`](#trainerramen_handwritten_trainerrs手写策略测试壳) — 2
- [采样器](#采样器) — 13
- [RNG 受控重构](#rng-受控重构) — 8
- [输出 / 日志](#输出--日志) — 19 个
  - [`output/decision.rs`](#outputdecisionrs决策评分分解) — 6
  - [`output/view.rs`](#outputviewrs视图层) — 4
  - [`output/turn_flow.rs`](#outputturn_flowrs回合流程渲染) — 4
  - [`output/decision_log.rs`](#outputdecision_logrs决策日志落盘) — 4
  - [`output/diagnostic.rs`](#outputdiagnosticrsdiag-宏) — 1
- [配置 / 数据加载](#配置--数据加载) — 18 个
  - [`gamedata/config.rs`](#gamedataconfigrs配置结构与合并) — 12
  - [`gamedata/tests`](#gamedatatestsgamedata-加载) — 4
  - [`gamedata/event.rs`](#gamedataeventrs事件数据加载) — 2
- [基础游戏杂项](#基础游戏杂项) — 6 个
  - [`game/uma.rs`](#gameumarsuma-结构) — 3
  - [`game/support_card.rs`](#gamesupport_cardrs支援卡) — 1
  - [`game/inherit.rs`](#gameinheritrs继承值) — 1
  - [`game/tests`](#gametests基础类型) — 1
- [工具与随机数](#工具与随机数) — 16 个
  - [`utils.rs`](#utilsrs配置加载校验) — 8
  - [`rng.rs`](#rngrs顶层-rng-工具) — 8
- [基准测试](#基准测试)（bench_base / bench_compositions 运行说明）— 9
- [神经网络](#神经网络) — 1

**lib 状态**：312 passed / 3 failed（PR #25 改 5 维上限后 3 个 hardcode baseline 过期：test_yearly_observability_full_game_and_csv / test_ramen_three_stage_action_unchanged / test_combined_gate_off_full_game，用户确认暂不处理）/ 3 ignored

---

## 拉面杯

### `game.rs`（阶段流转与集成）

**初始化 / 基础**
- `test_ramen_game_newgame` — 新建游戏合法性
- `test_ramen_newgame_requires_new_friend` — 缺新友人卡时拒绝开局

**训练参数 / 数值**
- `test_train_param_decomposition` — 训练参数拆解正确性
- `test_random_distribution_training_value` — 随机分配下训练数值计算
- `test_ramen_deyilv_includes_scenario_bonus` — 得意率叠加剧本加成
- `test_random_event_generation` — 随机事件生成

**端到端 / 集成**
- `test_ramen_game_full_loop` — 拉面杯基础闭环（带日志）
- `test_ramen_silent_loop` — 拉面杯基础闭环（关日志）
- `test_manual_trainer_full_game` — ManualTrainer 完整流程
- `test_manual_trainer_hint_special_path` — 第3年 hint_special 路径不崩溃

**决策路径**
- `test_three_stage_decision_flow` — 三阶段（RamenSelect→SpecialSelect→Train）衔接
- `test_combined_decision_path_skips_special_select` — 合并决策跳过 SpecialSelect
- `test_combined_decision_path_no_ramen` — 合并决策不吃面路径
- `test_combined_decision_invalid_targets_rejected` — 合并决策拒绝非法 targets
- `test_three_stage_path_unaffected_by_combined_flag` — combined flag 不污染三阶段路径

**RMJ / 剧本事件**
- `test_select_rmj_choice_by_result` — RMJ 事件按 result 选择分支
- `test_rmj_event_year` — RMJ 事件按回合映射年份
- `test_rmj_event_apply_success` — RMJ 成功 apply 正确
- `test_rmj_event_apply_fail` — RMJ 失败 apply 正确
- `test_rmj_event_immediate_apply_at_turn_23` — 第1年 RMJ 当回合立即触发
- `test_scenario_pt_reset_after_rmj` — RMJ 结算后 scenario_pt 归零
- `test_generate_events_uma_debut` — 马娘登场事件生成（turn=0）
- `test_generate_events_classic_newyear` — 经典新年事件（turn=24）
- `test_generate_events_ancient_newyear` — 古马新年事件（turn=48）
- `test_add_mandatory_events_ticket_at_48` — turn=48 抽签事件
- `test_add_mandatory_events_ending_at_77` — turn=77 结局事件

**超级拉面**
- `test_super_ramen_base_effect_vital_motivation` — 基础效果（体力+干劲）每回合生效
- `test_super_ramen_saihou_one_time_only` — 赛后加成仅 turn=72 一次性

**hint_special**
- `test_hint_special_inactive_without_ramen` — 未吃面时不激活
- `test_hint_special_inactive_year1_2` — 第1/2年不激活
- `test_hint_special_active_year3` — 第3年吃面后激活
- `test_hint_special_only_at_listed_trains` — 仅在配置的训练位置生效
- `test_hint_special_inactive_low_card_types` — 支援卡种类<4 时不激活

**第3年地区策略**
- `test_ramen_region_strategy_fixed_skips_enumeration` — 第3年 Fixed 策略跳过枚举
- `test_year1_2_always_all_regardless_of_strategy` — 第1/2年 Fixed 不生效

**回合菜单约束**
- `test_skip_ramen_select_for_turn_0_1_and_super_ramen` — 回合 0-1/超级拉面短路 Distribute→Train

**人头 / 卡组下标解耦（2026-08-23 起）**
- `test_count_training_persons_by_type` — 按 `PersonType` 判定（替代硬编码下标），负数/越界一并不计
- `test_count_training_persons_onsen_unchanged` — onsen 路径逐位不变守门
- `test_person_deck_index_mapping_full_game` — 完整局人头与卡组下标映射一致

**分身 / 缺席分配（2026-08-23 起）**
- `test_distribute_person_two_stage_absent` — 两步算法：先过滤合法落点再均匀抽
- `test_absent_recorded_and_npc_always_present` — 缺席名单进入 RamenState，NPC 始终在场
- `test_absent_weight_by_type` — 按 PersonType 加权抽取

**阶段边界（2026-08-26 起）**
- `test_non_turn2_has_no_begin_after_region_select` — 非回合 2 不走 `BeginAfterRegionSelect`
- `test_turn2_stage_sequence` — 回合 2 阶段流 `Begin → RegionSelect → BeginAfterRegionSelect → Distribute`
- `test_year1_region_select_uses_full_enumeration` — 第 1 年地区选 all 枚举
- `test_year3_fixed_list_actions_single_candidate` — 第 3 年 Fixed 策略只生成一个动作

**逐年归档 / RMJ 观测**
- `test_rmj_archives_yearly_counters_before_reset` — RMJ 结算前先按年归档 scenario_pt/eat_count/region

**分身 / 地区 / 训练分布**
- `test_region_clones_absent_priority` — 地区分身缺席优先
- `test_region_clones_per_train_semantics` — 地区分身按训练位置语义
- `test_training_buff_person_deck_mapping` — 训练 buff 人头-卡组下标映射

### `rules.rs`（诀窍、吃面与地区）

**吃面消耗**
- `test_consume_for_ramen` — 吃面正常消耗
- `test_consume_for_ramen_errors` — 吃面错误（配方/隐藏风味/库存不足）
- `test_can_make_ramen` — 能否做面判定

**诀窍 / 诀窍槽**
- `test_feeling_overflow` — 诀窍总数超过 10 时淘汰最早
- `test_gauge_overflow` — 诀窍槽溢出也只加 1 诀窍并清零
- `test_gauge_base_distribution` — 按配方比例分配 base_sum
- `test_fill_gauge_after_train_normal` — 训练后填充（普通回合）
- `test_fill_gauge_after_train_xiahesu_max` — 训练后填充（夏合宿全 MAX）
- `test_fill_gauge_after_non_train_normal` — 非训练后填充
- `test_fill_gauge_after_non_train_xiahesu` — 非训练后填充（夏合宿全 MAX）
- `test_fill_gauge_after_non_train_xiahesu_partial` — 夏合宿非训练但部分 MAX
- `test_get_turn_special_feeling` — 固定回合隐藏风味加成表

**特殊目标枚举**
- `test_list_special_targets_full_stock_sapporo_9` — 库存富余候选（札幌 [2,2,1]）
- `test_list_special_targets_min_needed_a3b1c1` — 库存有缺口（A 缺3/B 缺1/C 缺1）
- `test_list_special_targets_impossible` — 库存完全不够
- `test_list_special_targets_no_special_feeling` — 无隐藏风味时只生成 0/0/0 候选
- `test_list_special_targets_recipe_with_zero_dim` — 配方含 0（如 [2,3,0]）
- `test_list_special_targets_sorted_ascending` — 候选按 sum(t) 升序

**训练加成**
- `test_train_feeling_bonus` — 训练角标加成

**PT 增量**
- `test_calc_ramen_pt_gain` — 吃面 PT 增量公式

**RMJ**
- `test_check_rmj` — RMJ 三种 result（Fail/Success/GreatSuccess）判定

**地区**
- `test_get_region_range` — 按年份取可选地区 ID 范围
- `test_get_region_clone_trains` — 地区分身位置
- `test_get_super_ramen_clone_train_options` — 超级拉面分身位置选项
- `test_calc_region_bonus` — 地区词条加成按 PT 档位
- `test_validate_region_selection` — 地区组合合法性校验

**NPC**
- `test_npc_chara_ids` — NPC chara_id 集合正确
- `test_special_targets_sum_invariant` — `special_targets` 之和 ≤ 2 不变量
- `test_special_targets_enumeration_is_within_ten` — special_targets 候选枚举 ≤ 10 个
- `test_combined_ramen_actions_peak` — 合并候选高峰与 candidate 计数对齐

### `effects.rs`（训练数值与得意率）

**训练数值计算**
- `test_apply_training_value_status` — 属性训练下层/上层数值
- `test_apply_training_value_pt` — PT 训练数值计算
- `test_apply_training_value_upper_limit` — 上层数值上限约束
- `test_apply_training_value_lower_cap` — 下层数值上限100

**训练效果来源**
- `test_calc_effect_pt_only` — PT 档位效果
- `test_calc_effect_with_eating` — 普通吃面 buff 叠加
- `test_calc_effect_rmj_success` — RMJ 成功加成
- `test_calc_effect_super_ramen` — 超级拉面效果
- `test_calc_effect_super_ramen_with_split` — 超级拉面 + 分身
- `test_calc_effect_non_shining` — 非友情训练无 youqing

**剧本得意率**
- `test_calc_scenario_deyilv_normal_pt_only` — 普通回合 PT 档位得意率
- `test_calc_scenario_deyilv_normal_with_rmj_success` — 普通回合 + RMJ 成功
- `test_calc_scenario_deyilv_normal_with_rmj_fail` — 普通回合 + RMJ 失败
- `test_calc_scenario_deyilv_super_ramen` — 超级拉面回合得意率
- `test_calc_scenario_deyilv_super_ramen_rmj_fail` — 超级拉面 + RMJ 失败

### `action.rs`（动作枚举与列表）

**枚举与显示**
- `test_ramen_action_display` — RamenAction 的 Display 输出格式
- `test_ramen_action_properties` — `is_eating_ramen` / `base_operation` getter
- `test_combined_select_keeps_targets_when_eating` — 合并决策吃面时保留 targets
- `test_combined_select_normalizes_targets_when_no_ramen` — 不吃面时 targets 归零
- `test_train_gauge_uses_actual_npc_count` — 训练诀窍槽用实际 NPC 人数
- `test_region_select_archives_explicit_year_idx` — 地区选择按显式 year_idx 归档
- `test_super_ramen_select_list_and_apply` — 超级拉面选面列表与落地
- `test_super_ramen_clones_include_friend_card` — 超级拉面分身含友人卡
- `test_super_ramen_clones_friend_priority_beats_greedy_starvation` — 友人卡分身优先于贪心饿死
- `test_super_ramen_clones_decoupled_from_parent_stream` — 超级拉面分身流与本体解耦
- `test_clone_placement_full_train_and_npc_eviction` — 训练满员时分身挤 NPC

**列表生成**
- `test_list_ramen_choices` — 面选择枚举（含不吃）
- `test_list_operations` — Operation 列表（含夏合宿/友人/治病等条件）
- `test_list_all_actions` — 完整动作列表（吃面×操作笛卡尔积）
- `test_list_train_actions_no_ramen_field` — Train 阶段动作不携带 ramen/special_targets
- `test_list_ramen_select_actions_full` — 拉面选择阶段（3 面都可选）
- `test_list_ramen_select_actions_no_available` — 拉面选择阶段（无可选面）
- `test_list_special_select_actions_uses_special_targets` — 隐藏风味选择阶段
- `test_list_combined_ramen_select_actions_full` — 合并决策完整候选
- `test_list_combined_ramen_select_actions_no_available` — 合并决策无可选
- `test_get_available_ramens` — 当年可用面判定

### `events.rs`（友人事件与剧本事件）

- `test_event_ids` — 事件 ID 表正确
- `test_turn_special_feeling` — turn→特殊隐藏风味数量映射
- `test_friend_event_state_lifecycle` — 友人事件状态机（首次/点击/解锁/出行）
- `test_friend_visibility` — 友人可见性
- `test_assign_train_feeling_type` — 训练角标分配每种至少1次

### `policy.rs`（固定策略与手写策略核心）

**固定策略**
- `test_fixed_region_selection` — 各年份固定地区选择
- `test_fixed_super_ramen_selection` — 超级拉面固定选项二

**手写策略核心（RamenPolicy）**
- `test_gate_ill_clinic` — 守门：生病必治病
- `test_gate_vital_low_rest` — 守门：体力低必休息
- `test_gate_motivation_low_outing` — 守门：心情低必外出
- `test_train_selector_deterministic` — 健康局面确定性选训练（两次一致）
- `test_special_selector_min_hidden` — SpecialSelect 最省隐藏风味
- `test_event_selector_higher_value` — 事件选效果总值高者
- `test_region_selector_valid_and_deterministic` — 地区组合可打分且确定性

**自选比赛守门 / 打分自洽性**
- `test_remaining_race_slots` — 区间内剩余可比赛回合数（按当前回合裁剪，排除回合 11-12 与 URA 段）
- `test_free_race_gate` — 硬守门四场景（宽裕不干预 / 紧张强制比赛 / 已达标 / 无要求马娘）
- `test_free_race_gate_giveup_recorded` — 摆烂判定进入观测
- `test_free_race_gate_quiet_after_done` — 已达标后不再守门
- `test_free_race_gate_skips_nonqualified_turn` — 等级不满足的回合跳过比赛守门
- `test_free_race_gate_without_race_candidate` — 候选表不含「比赛」时返回 None 而非越界 panic
- `test_breakdown_sums_to_score` — 打分 breakdown 各项之和等于 score（决策日志自洽）
- `test_status_rate_is_linear` — `status_rate` 线性生效（防止重复相乘成平方）
- `test_free_race_gate_oguri_two_intervals` — 小栗帽 100603 专项：两段区间从 DB 正确读出、
  限 G1 使第二段可比赛回合 12→7、两段守门均按缺口提前触发并返回「比赛」

**自由比赛真实收益（2026-08-25）**
- `test_score_race_panel_properties` — 真实收益管道：无比赛 0 分 / race_bonus 乘算 / 折扣生效
- `test_score_race_skips_nonqualified_turn` — 等级不满足的回合给真实收益（不强制）

**超级拉面 / 地区选择（2026-08-23+）**
- `test_decide_super_ramen_finds_option_two` — 超级拉面选选项二
- `test_region_build_sensitivity` — 权重默认 0 时选区保持稳定
- `test_region_selection_per_build` — 按 build 列出三年选区占比

**race_turn 守门**
- `test_race_turn_qualified` — race_turn 等级满足守门

### `logging_trainer.rs`（决策日志包装）

- `test_logging_trainer_records_full_game` — 完整局决策记录覆盖（三阶段/事件/地区选择）
- `test_reproducible_same_seed` — 同 seed 两次整局决策序列与评分一致（可复现性）

### `ramen_handwritten_trainer.rs`（手写策略测试壳）

- `test_handwritten_full_game` — 完整 77 回合跑通（评分/RMJ/吃面数输出）
- `test_handwritten_reproducible` — 同 seed 两次整局评分一致

### `features.rs`（拉面局面特征编码器）

NN 管线用：把局面编码为定长向量（global / cards / persons 三段），含成长率 / 五维上限 / 人头分支。

- `test_encode_deterministic` — 同局面两次编码逐位一致
- `test_encode_newgame` — 开局局面编码符合预期
- `test_encode_sampled_positions` — 抽样局面编码覆盖
- `test_dim_constants_consistent` — 维度常量与编码同步
- `test_card_person_cross_lookup` — 卡组 / 人头交叉查表
- `test_split_person_multi_hot` — 人头分桶 multi-hot 编码
- `test_status_bonus_reaches_features` — 五维加成传入特征
- `test_stage_num_reserve_slots` — 阶段编号保留槽
- `test_year1_region_root_features` — 第 1 年地区根特征

### `rng_consistency.rs`（RNG 受控重构集成测试）

跨策略 / 跨脚本 20 回合角标 / 分布 / 固定流消费逐位一致。

- `test_layer2_cross_strategy_consistency` — 跨策略层 2 一致性
- `test_layer3_clone_isolation` — 层 3 克隆隔离
- `test_layer3_stream_isolation` — 层 3 流隔离
- `test_layer3_turn_reset_isolation` — 层 3 回合重置隔离

### `state.rs`（剧本状态）

- `test_region_archive_year_idx_not_current_year` — 地区归档按年份硬编码而非 `current_year()`（回合 23/47 归档时 year_idx 用 0/1/2）

---

## 基础游戏（14 个）

### `game/base/mod.rs`（基础游戏与五维上限）

- `test_explain` — BaseGame explain
- `test_newgame` — 新建基础游戏
- `test_can_self_race_bounds` — 自选比赛边界（13-71 允许，URA 回合禁止）
- `test_can_friend_outing_bounds` — 友人出行边界（解锁/回合 <72/次数未用完）
- `test_apply_event_friend_bonus_integration` — 友人卡事件加成集成（体力/PT/状态）
- `test_apply_event_no_friend_bonus_backward_compatible` — 无加成时旧行为兼容
- `test_apply_friend_bonus_no_bonus` — 加成参数为 None 时不动
- `test_apply_friend_bonus_other_fields_unchanged` — 加成只改指定字段
- `test_apply_friend_bonus_status_pt` — 加成作用于五维 / PT
- `test_apply_friend_bonus_vital` — 加成作用于体力
- `test_apply_friend_bonus_vital_negative_not_affected` — 体力已 0 不再扣
- `test_newgame_status_limit_is_scenario_base_plus_inherit` — **五维上限 = 剧本基值 + 开局继承（PR #25 三剧本守门）**

### `game/base/basic.rs`（BasicGame）

- `test_newgame` — 新建 BasicGame
- `test_view_default` — Game trait 默认 view 实现

### `game/uma.rs`（Uma 结构）

- `test_uma` — Uma 基础结构
- `test_win_races` — 比赛胜场计算
- `test_score_parts_matches_calc_score` — `Uma::score_parts()` 与 `calc_score()` 一致

### `game/support_card.rs`

- `test_support` — 支援卡基础结构

### `game/inherit.rs`

- `test_inherit` — 继承值生成

### `game/mod.rs`

- `test_friend` — FriendState 基础结构

---

## 搜索（34 个）

### `search/flat_search.rs`（扁平蒙特卡洛搜索，20 个）

**根搜索与可复现性**
- `test_search_reproducible_same_seed` — 同 seed 两次搜索结果一致
- `test_search_seed_actually_used` — 搜索种子真实消耗
- `test_ramen_root_search_reproducible` — 拉面根搜索可复现
- `test_ramen_root_search_seed_used` — 拉面根搜索种子消耗
- `test_ramen_simulate_common_ignores_reseed` — 拉面 simulate_common 不读 crn_stage_reseed
- `test_simulate_common_matches_dual_seed_wrapper` — 双种子 wrapper 与 simulate_common 等价

**UCB / 候选顺序**
- `test_search_ucb_reproducible` — UCB 路径同 seed 两次一致
- `test_search_ucb_order_sensitivity` — UCB 候选重排时统计差异
- `test_search_invariant_to_action_order` — 非 UCB 路径候选顺序不变
- `test_ucb_first_group_clamps_to_search_n` — UCB 首组越预算守门

**CRN / 配对**
- `test_onsen_crn_reseed_changes_result` — onsen 阶段重播种开关必须改变分数向量
- `test_crn_pairing_gain` — CRN 共享 rule_master 时配对增益
- `test_crn_pairing_gain_ramen` — 拉面 CRN 配对增益
- `test_crn_pairing_gain_ramen_small` — 拉面小规模 CRN 配对增益
- `test_crn_pair_alignment_keeps_original_j` — 失败样本按原序号配对
- `test_ramen_crn_seed_topology` — 拉面 CRN 种子拓扑

**合并动作 / 三阶段**
- `test_ramen_combined_action_full_game_smoke` — 合并动作完整局烟雾测试
- `test_ramen_combined_action_preserves_targets` — 合并动作保留 targets
- `test_ramen_combined_action_rejects_illegal_targets` — 合并动作拒绝非法 targets
- `test_ramen_three_stage_action_unchanged` — 三阶段动作逐位不变（含 PR #25 baseline）

### `search/seeds.rs`（rollout 种子派生，6 个）

- `test_distinct_root_distinct_sequence` — 不同根种子派生不同序列
- `test_from_rng_follows_entry_seed` — `from_rng` 跟随 entry_seed
- `test_seed_at_deterministic` — `seed_at(j)` 确定性
- `test_seed_at_distinct_per_rollout` — rollout 序号派生唯一
- `test_stage_seed_distinct_per_rollout` — 同阶段内 rollout 唯一
- `test_stage_seed_distinct_per_turn_and_stage` — 跨回合×阶段唯一

### `search/ramen_terminal.rs`（拉面终局观测，5 个）

- `test_dim_keys_frozen` — FROZEN_DIM_KEYS 契约
- `test_visit_covers_all_dims` — 访问覆盖所有冻结维度
- `test_ramen_terminal_from_game` — 从 game 提取终局统计
- `test_threshold_must_be_reduced_per_rollout` — 阈值类维度按 rollout 折算
- `test_gap_spread_separates_balance` — gap_spread 区分均衡程度

### `search/terminal.rs`（通用终局统计，2 个）

- `test_no_terminal_is_zst` — `NoTerminalStats` 是 zero-sized
- `test_moment_result` — `MomentResult` 累加行为

### `search/config.rs`（搜索配置契约，1 个）

- `test_new_game_config_follows_crn_stage_reseed` — `crn_stage_reseed` 跟随 GameConfig

---

## 训练员（31 个）

### `trainer/ramen_mcts_trainer.rs`（拉面 MCTS 训练员，15 个）

**阶段门控与 fallback**
- `test_combined_gate_off_full_game` — 门控全关 = 推荐策略逐位一致（含 PR #25 baseline）
- `test_stages_none_matches_recommended` — 门控全关搜索结果 = REC（**REC fallback 守门，419f9db**）
- `test_mcts_train_only_full_game` — train_only 单局跑通
- `test_region_gate_three_years` — 地区阶段三档门控

**阶段解析**
- `test_search_stages_parse` — `RamenSearchStages::parse` 全场景
- `test_combined_default_on` — 默认开合并动作

**搜索阶段根测试**
- `test_year1_region_is_searched` — 第 1 年地区纳入搜索
- `test_year1_region_search_root_smoke` — 第 1 年地区搜索根烟雾
- `test_super_ramen_gate_searches_once` — 超级拉面搜索一次
- `test_super_ramen_search_root_smoke` — 超级拉面搜索根烟雾

**合并 / 缓存**
- `test_combined_on_skips_special_search` — 合并开启时 SpecialSelect 多数走缓存命中
- `test_combined_cache_used_when_special_gate_off` — special gate 关闭时缓存命中

**可复现性 / RNG**
- `test_mcts_reproducible` — MCTS 整局可复现
- `test_root_action_uses_strategy_stream` — 根动作走策略流

**观察输出**
- `test_terminal_breakdown_demo` — verbose 终局差异演示

### `trainer/local_ramen_trainer.rs`（本地手写策略，12 个）

- `recommended_ramen_new_mechanisms_enabled` — REC 新机制全开（吃面联动 / 体力门限 / 友人节奏 / 动态属性平衡）
- `recommended_ramen_uses_025_friend_pacing` — REC 友人节奏系数 0.25 生效
- `recommended_region_select_year1_runs_policy` — REC 第 1 年地区走 policy
- `cap_discount_ratio_behavior` — cap_discount 按比例缩放
- `ramen_weak_train_boost_effect` — 弱位训练加成生效
- `train_coupling_bonus_on_eating` — 训练-吃面耦合加成
- `eat_covered_train_gate_blocks_mismatched_ramen` — 吃面覆盖门控拒错配面
- `eat_guarantee_value_on_risky_train` — 风险训练吃面价值保底
- `friend_hidden_starve_and_overflow_guard` — 友人隐藏风味饥饿 + 溢出守门
- `friend_future_hidden_supply` — 友人未来隐藏风味供给
- `local_single_candidate_breakdown_and_for_rollout` — 单候选 breakdown + for_rollout
- `microbench_top_fns` — **手写逻辑热点 microbench（d10872a perf 工具）**

### `trainer/logging_trainer.rs`（决策日志包装，2 个）

- `test_logging_trainer_records_full_game` — 完整局决策记录覆盖
- `test_reproducible_same_seed` — 同 seed 两次决策序列一致

### `trainer/ramen_handwritten_trainer.rs`（手写策略测试壳，2 个）

- `test_handwritten_full_game` — 完整 77 回合跑通
- `test_handwritten_reproducible` — 同 seed 评分一致

---

## 采样器（`sampler.rs`，13 个）

NN 管线 Phase 2：分层的采样空间 + 按工作项序号确定性派生 + 轨迹扰动。

- `test_spec_deterministic` — 同 spec 两次派生一致
- `test_spec_covers_turn_range_and_decks` — 覆盖回合区间与卡组
- `test_sampled_position_is_advanceable` — 抽样局面可推进
- `test_sampled_position_feeds_search` — 抽样局面喂入搜索
- `test_sample_seed_actually_used` — 种子真实消耗
- `test_sample_reproducible` — 同种子两次抽样一致
- `test_sample_covers_all_turns` — 全回合覆盖
- `test_gen1_space_size` — gen1 空间大小
- `test_gen1_space_excludes_chara_conflict` — 排除角色冲突
- `test_gen1_decks_wellformed` — gen1 卡组合法
- `test_epsilon_perturbs_trajectory` — 扰动轨迹
- `test_epsilon_out_of_range_rejected` — 扰动参数越界拒收
- `test_combinations_boundaries` — 组合边界

---

## RNG 受控重构（`rng.rs`，8 个）

- `test_deterministic` — 同种子产出相同
- `test_counter_continues` — 计数器跨函数保持
- `test_clone_independent` — 克隆独立
- `test_typed_streams_work` — 类型化流工作
- `test_stream_tags_isolated` — TAG 隔离
- `test_masters_differ` — 主种子派生有别
- `test_fork_local_stream` — 派生局部流
- `test_additive_no_xor_collision` — 加法派生无碰撞

---

## 输出 / 日志（19 个）

### `output/decision.rs`（决策评分分解，6 个）

- `test_default_is_zero_index` — 默认零索引
- `test_from_index_and_score` — 按索引与分数构造
- `test_from_index_minimal` — 最小信息构造
- `test_serde_json_value_conversion` — 序列化为 JSON value
- `test_serde_roundtrip_full` — 完整 serde 往返
- `test_serde_roundtrip_minimal` — 最小 serde 往返

### `output/view.rs`（视图层，4 个）

- `test_default_view` — 默认 view
- `test_with_scenario_only_sets_scenario` — 仅设 scenario 字段
- `test_view_as_serde_json_value` — 序列化为 JSON value
- `test_serde_roundtrip` — serde 往返

### `output/turn_flow.rs`（回合流程渲染，4 个）

- `test_turn_output_baseline` — turn 输出基线
- `test_distribution_colors` — 分布彩色渲染
- `test_vital_color` — 体力颜色分级
- `test_verbose_demo` — verbose 演示

### `output/decision_log.rs`（决策日志落盘，4 个）

- `test_csv_escape` — CSV 字段转义（逗号/引号/换行）
- `test_csv_row_and_header` — 单行序列化 + 表头格式
- `test_empty_log` — 空日志 CSV 输出
- `test_save_to_roundtrip` — 落盘与读取往返

### `output/diagnostic.rs`（diag! 宏，1 个）

- `test_diag_expands_to_info` — 编译期展开为 `log::info!`（开启时）

---

## 配置 / 数据加载（18 个）

### `utils.rs`（配置加载校验，8 个）

- `test_validate_game_config_scenario_enum` — scenario 枚举校验
- `test_validate_game_config_trainer_enum` — trainer 枚举校验
- `test_validate_game_config_ramen_region_fixed_length` — Fixed 策略下 `ramen_region_fixed` 长度校验
- `test_resolve_default_config_path` — 默认配置路径解析
- `test_resolve_user_config_path_points_to_workspace_root` — 用户配置路径指向 workspace 根
- `test_missing_user_config_keeps_production_mcts` — 缺文件兜底不践踏 search_n / radical_factor_max
- `test_default_config_ramen_region_fixed` — `default_config.toml` 顶层 ramen_region 字段解析
- `test_override_config_trainer_overrides_default` — **trainer 字段扩展守门（2026-08-28）**

### `gamedata/config.rs`（配置结构与合并，12 个）

- `test_override_merge_all_none_keeps_default` — 全 None 兜底保留 default
- `test_override_merge_partial_overrides` — 部分字段覆盖
- `test_override_config_denies_unknown_fields` — `[config_override]` 未知字段报错
- `test_override_config_parses_without_mcts_or_bogus` — 无 `[mcts]` 段或带错键仍可解析
- `test_mcts_override_denies_unknown_fields` — `[mcts]` 未知字段报错
- `test_mcts_override_omitted_section_keeps_all_twelve` — 整段省略保留全部 12 字段
- `test_mcts_override_partial_fields_apply` — 部分字段覆盖生效
- `test_mcts_override_daily_path_keeps_production` — 日常路径覆盖不践踏
- `test_top_level_region_override_takes_effect` — 顶层 ramen_region 字段覆盖
- `test_production_default_searches_ramen_stage` — 生产 default 含 `ramen` 阶段
- `test_scenario_status_limit_base_contract` — **剧本基值字面量契约（PR #25）**
- `test_status_final_score_saturates_out_of_range` — **status_final_score 越界饱和（PR #25）**

### `gamedata/mod.rs`（gamedata 加载，4 个）

- `test_uma_data` — 马娘数据加载
- `test_support_data` — 支援卡数据加载
- `test_consts` — 常量加载
- `test_turn_mask` — 回合掩码

### `gamedata/event.rs`（事件数据加载，2 个）

- `test_load_and_explain_all_events` — 加载并 explain 全事件
- `test_load_and_explain_ramen_events` — 加载并 explain 拉面事件

---

## 神经网络（1 个）

- `neural/evaluator.rs::test_random_evaluator_send_sync` — 随机评估器线程安全

---

## 基准测试（`bin/bench_base.rs`）

固定种子批量跑批，产出 RandomTrainer 基线分布（分数/PT/RMJ/耗时）与决策轨迹，
用于量化手写策略的改进（对应手写策略计划 §8「先立地基」）。

```bash
# 默认读取 workspace 根 bench_config.toml（runs=20, seed=42）
cargo run --release --bin bench_base

# 自定义局数/种子/开启决策日志/输出目录（CLI 覆盖 config）
cargo run --release --bin bench_base -- --runs 100 --seed 7 --log --out logs
```

- 参数：`--runs N` 局数、`--seed S` 基础种子（第 i 局 = seed+i）、`--log` 落盘决策日志（默认关）、`--out DIR` 输出目录
- 产出（默认 `logs/`）：`bench_base_results.csv`（每局一行：seed/分数/rank/五维/PT/RMJ/吃面数/耗时）+ 汇总统计
  （分数 mean/median/min/max/std、按阶段分组的决策耗时、吞吐）；`--log` 时另产出 `bench_base_decision_<seed>.csv`
- 可复现性：同一参数下游戏结果完全一致（决策 RNG 与规则层 `internal_rng` 均由 seed 派生；`elapsed_ms` 属运行耗时，允许波动）
- 性能基线（2026-08-21 实测）：RandomTrainer 单局 ~1.2ms，吞吐 ~815 局/s——手写策略须保持 O(候选数) 简单才有 rollout 接入意义

---

### `bin/bench_compositions.rs`

固定种子遍历五种普通支援卡各 0..=3 张、合计 5 张再加固定友人的全部 101 种构成，输出评分、五维、训练技能 PT、RMJ 和友人出行聚合 CSV。运行设施复用 `umasim::bench`（`bin/bench_base.rs` 同）。

```bash
cargo run --release --bin bench_compositions -- --runs 100 --seed 42 --trainer handwritten --out logs/bench_compositions.csv
```

- 代表卡选择：各类型取最新 5 张满破 SSR 作候选池，跳过满破面板和值（友情+干劲+训练）<70 的弱卡，按 card_id 倒序取 3 张；`--min-panel N` / `--pool-size N` / `--pick N` 可调，`--cards-file cards.toml` 手动指定兜底（每类型满破 idrank 列表）
- `test_enumerate_all_101_compositions` — 严格验证合法构成总数为 101，且每种合计 5 张、单类型不超过 3 张
- `test_build_all_composition_decks` — 验证全部构成都生成 5 张普通卡 + 1 张固定友人
- `umasim::bench` 模块测试（4 个）：seed 双 RNG 可复现 / summarize 统计 / percentile 分位 / 真实 cardDB 默认参数选卡集成验证

## 未来缩减参考（规则固化后讨论）

**前提**：拉面杯目前仍在重构期，公式随时可能调整；规则固化后可重新评估公式测试的密度。

**公式测试的价值三层次**：
1. **回归保护** — 重构期防止破坏；规则固化后价值下降（停止改动后无破坏）
2. **文档化** — 把预期行为固化在代码里；规则固化后可由正式文档替代
3. **调试辅助** — 快速定位数值问题；规则固化后价值保留

**建议的缩减方向**（"保留 happy path + 关键边界，删除纯中间态"）：

| 系列 | 当前 | 可压缩到 | 缩减点 |
|------|------|---------|--------|
| `test_calc_scenario_deyilv_*` | 5 | 3 | normal_with_rmj_success + normal_with_rmj_fail 合并；super_ramen 单独保留 |
| `test_apply_training_value_*` | 4 | 2 | status + lower_cap 合并；pt + upper_limit 合并 |
| `test_calc_effect_*` | 6 | 3 | pt_only + eating 合并；rmj_success 单独；super_ramen + super_ramen_with_split 合并 |
| `test_list_special_targets_*` | 6 | 3-4 | 保留 full_stock + min_needed + impossible + sorted；no_special_feeling + recipe_with_zero_dim 可视为 full_stock 变体 |
| `test_fill_gauge_*` | 5 | 3 | train_normal + non_train_normal 合并；各自 xiahesu 路径合并；partial 单独保留 |

**预估可缩减 15-20 个**（121 → 约 100-105）。

**应保留**：
- 端到端（3 个）：整体回归必备
- 决策路径（5 个）：三阶段/合并决策是核心设计
- RMJ/事件（10+ 个）：关键业务流程
- `hint_special_*`（5 个）：每个 case 不同，全保留
- 每个公式函数至少 1 个 happy path

**风险**：游戏更新（数据库调整、剧情加强）时边界 case 测试可能重新需要；公式逻辑本身不会变，风险可控。

**讨论结论（2026-08-20）**：当前不执行——重构期公式随时可能改动，现在缩减可能需要后续重新加；待规则固化后重新评估。