//! 评分解释层：复刻并增强 URA（UmamusumeResponseAnalyzer）的评分预测功能。
//!
//! # 名词先用大白话解释
//!
//! - **URA**：社区工具「UmamusumeResponseAnalyzer」，能读游戏网络包，把育成结束时的
//!   五维、技能点、技能列表翻译成"最终评分 + 技能购买推荐表"。
//! - **评价点（评价分）**：游戏给马娘育成结果打的分数，就是结算画面那个大数字。
//! - **技能点（PT）**：买技能用的货币。育成里攒下来，结算前花掉。
//! - **评分增量（grade）**：买一个技能能让总分涨多少（官方数据，存在游戏库 skill_data
//!   表的 grade_value 列）。
//! - **性价比**：URA 的算法是「评分增量 ÷ 价格」，数值越大越划算。
//! - **US1-US9**：官方评价档次表里 6 万分以上的一段（见 gamedata/constants.json 的
//!   rank_scores/rank_names，与本仓库 `get_rank_name` 同源）。
//!
//! # 两种评分口径（本模块的核心，先读这段再看代码）
//!
//! 1. **URA 口径（通用育成结算）**：
//!    `总分 = 属性评分 + 已学技能评分 + 即将学习技能评分`
//!    属性评分查 URA 开源仓库的 StatusToPoint 表（0..2500 档，1200+ 区间权重更高）。
//!    这个口径下技能点本身不算分，花掉换技能就等于把技能点"兑现"成了评分。
//!
//! 2. **本仓库结算口径（新口径 2026-09-17，终局买技能）**：见 `game/uma.rs` 的 `calc_score`：
//!    `总分 = 属性分(URA表) + (固有510 + Σ买到技能Grade) + 结余PT × 2`
//!    育成全程**不学技能**只攒 Pt；育成结束时按「净增分 = Grade − 价格×2 > 0」
//!    从高到低贪心买入（×2 是 URA 截图对账的技能价格折算：1 技能点 = 2 评价点）。
//!    PT 项选择「结余折算」而非删除：买技能花掉的 PT 按同一折算率退出计分，
//!    「净增分 > 0 才买」的判据才与计分公式逐位自洽。
//!
//! 两口径的属性分在 0..1200 区间查的表完全一致（同一张官方表）；1200 之后本仓库
//! 新口径直接采用 URA 官方 2500 档（旧口径曾用 3399 档延拓，见 `LocalScoring::legacy_from_constants`）。
//! 差异见 `formula_compare()`，报告里也有一张对照表。
//!
//! # 数据来源（全部有据可查，无脑补）
//!
//! - `gamedata/skillDB.json`：从游戏 master.mdb 导出（skill_data + single_mode_skill_need_point
//!   + text_data category=47 技能名），导出脚本 tools/export_skilldb.py。
//! - `gamedata/hintDB.json`：master.mdb 的 single_mode_hint_gain（支援卡自带的
//!   hint 技能），用于「卡组 hint 口径」的可买技能集。
//! - `gamedata/ura_status_to_point.json`：URA 开源仓库
//!   <https://github.com/UmamusumeResponseAnalyzer/UmamusumeResponseAnalyzer>
//!   的 Database.cs 内嵌 StatusToPoint 表（2501 档）。
//! - 评价档次 rank_scores/rank_names：仓库原有 constants.json（与 URA 截图交叉验证
//!   一致：68369 分落在 [67700, 69000) = US4 档，距 US5 差 631 分，逐位吻合）。
//!
//! # 近似假设（诚实声明，不是实测）
//!
//! - URA 的技能折扣来自游戏实时推送的 hint 等级（0-5 级，0/10/20/30/35/40% off）。
//!   离线模拟没有实时 tips，本模块近似映射：卡组 hint 口径下，
//!   hint_gain_type=0（白圈）→ 打 9 折，hint_gain_type=1（金圈）→ 打 8 折。
//!   全池口径一律原价。
//! - 「可购买技能集」在全池口径下 = disable_singlemode=0 且有价格的技能
//! - 卡组 hint 技能（hintDB 里的 9081xxx 段）在现有数据源（旧版 master.mdb /
//!   上游 gamedata）中【没有名字、价格、评分】，按「禁止脑补」铁律不参与购买推荐，
//!   只用于跨卡组「技能面」对比（Jaccard 相似度）
//!   （rarity 1 普通 / 2 金），马娘固有（rarity 3+）不可买。卡组 hint 口径下 =
//!   卡组 6 张支援卡自带的 hint 技能。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::gamedata::{load_json, GAMECONSTANTS};
use crate::game::Uma;

/// URA 折扣表：hint 等级 0-5 对应的价格减免百分比（URA ApplyHint 原表，仅文档引用）。
pub const URA_HINT_OFF: [i32; 6] = [0, 10, 20, 30, 35, 40];

/// 本仓库拉面杯口径：每个 hint 等级折 6.5 技能点（constants.json 的 hint_pt_rate）。
/// 模块内的快照计算从 [`GAMECONSTANTS`] 运行时读取，不用裸数字。
pub const HINT_PT_RATE: f32 = 6.5;

/// skillDB.json 的一行（一个技能）。
#[derive(Debug, Clone, Deserialize)]
pub struct SkillEntry {
    /// 技能 id（游戏 skill_data.id）
    pub id: u32,
    /// 中文名（text_data category=47；缺省时空串）
    pub name_zh: String,
    /// 日文名（text_data category=47 原版）
    pub name_jp: String,
    /// 稀有度：1=普通（白）、2=金技能、3+ 固有/进化等（不可购买）
    pub rarity: i16,
    /// 技能组 id（同组=同族技能，如「末脚」与它的上位版）
    pub group_id: u32,
    /// 组内序号（URA 的 Rate，-1 表示未分组；用于识别上位技能）
    pub group_rate: i16,
    /// 评分增量（官方 grade_value：买了这个技能总分涨多少）
    pub grade: i32,
    /// 价格（官方 single_mode_skill_need_point.need_skill_point；0=查不到价格）
    pub cost: u32,
    pub icon_id: u32,
    pub disp_order: u32,
    /// 1=单模式（育成）禁用，0=可用
    pub disable_singlemode: u32,
}

impl SkillEntry {
    /// 展示名：中文优先，没有中文用日文，再没有就给 id
    pub fn display_name(&self) -> String {
        if !self.name_zh.is_empty() {
            self.name_zh.clone()
        } else if !self.name_jp.is_empty() {
            self.name_jp.clone()
        } else {
            format!("技能{}", self.id)
        }
    }
}

/// hintDB.json 的一行：某张支援卡自带的某个 hint 技能。
#[derive(Debug, Clone, Deserialize)]
pub struct HintEntry {
    /// hint 技能的 skill id
    pub skill_id: u32,
    /// 0=白圈 hint、1=金圈 hint（用于近似折扣档位）
    #[serde(default, alias = "hint_gain_type")]
    pub kind: u32
}

/// 技能池口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillPool {
    /// 全池：所有育成可买的普通/金技能（原价，不打折）
    All,
    /// 卡组 hint 口径：只买卡组 6 张支援卡自带的 hint 技能（带近似折扣）
    DeckHint
}

impl SkillPool {
    /// 从 CLI 字符串解析（`all` / `deck-hint`）
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "all" => Ok(SkillPool::All),
            "deck-hint" => Ok(SkillPool::DeckHint),
            other => anyhow::bail!("未知技能池口径: {other}（可选 all / deck-hint）")
        }
    }
}

/// 育成终局的评分相关快照（从 RamenGame 的 uma 字段抄出来，解耦模拟器类型）。
#[derive(Debug, Clone)]
pub struct UmaSnapshot {
    /// 五维终值 [速, 耐, 力, 根, 智]
    pub five_status: [i32; 5],
    /// 五维上限（剧本基值 + 继承上限）
    pub five_status_limit: [i32; 5],
    /// 已学技能评分之和（新口径：固有 510 + 终局买技能 ΣGrade）
    pub skill_score: i32,
    /// 剩余技能点（新口径：终局买入后 = 结余）
    pub skill_pt: i32,
    /// 全程获得的 hint 等级总数（每个 hint 等级折 6.5 技能点）
    pub total_hints: i32,
    /// 终局买技能账目：已买 Grade 合计（0 = 未结算或空买）
    pub bought_grades: i32,
    /// 终局买技能账目：已购总花费（skill_pt 已扣减）
    pub bought_cost: i32
}

impl UmaSnapshot {
    /// 从模拟器终局状态抄快照
    pub fn from_uma(uma: &Uma) -> Self {
        Self {
            five_status: uma.five_status,
            five_status_limit: uma.five_status_limit,
            skill_score: uma.skill_score,
            skill_pt: uma.skill_pt,
            total_hints: uma.total_hints,
            bought_grades: uma.bought_grades,
            bought_cost: uma.bought_cost
        }
    }

    /// 回推「终局买技能之前」的快照（推荐表重放用：对 before 重放确定性贪心，
    /// 得到的就是模拟器实际买入清单）。
    pub fn before_purchase(&self) -> Self {
        let mut before = self.clone();
        before.skill_score -= self.bought_grades;
        before.skill_pt += self.bought_cost;
        before.bought_grades = 0;
        before.bought_cost = 0;
        before
    }

    /// 总技能点 = skill_pt + total_hints × hint_pt_rate（向下取整，与
    /// `UmaGame::total_pt` 同式；比率运行时从 GAMECONSTANTS 读）。
    pub fn total_pt(&self) -> i32 {
        let rate = GAMECONSTANTS
            .get()
            .map(|c| c.hint_pt_rate)
            .unwrap_or(HINT_PT_RATE);
        (self.skill_pt as f32 + self.total_hints as f32 * rate).floor() as i32
    }
}

/// 饱和查表：负数按 0，超过表长按表末。表为空时返回 0（数据损坏时的唯一合理降级）。
pub fn lookup_saturated(table: &[i32], idx: i32) -> i32 {
    if table.is_empty() {
        return 0;
    }
    let i = (idx.max(0) as usize).min(table.len() - 1);
    table[i]
}

/// 本仓库结算口径的计分器（查表 + PT 折算率），运行时从 GAMECONSTANTS 构造。
///
/// 新结算口径（2026-09-17）下与 `UmaGame::calc_score` 完全同源：
/// 属性表用 URA StatusToPoint（与 calc_score 同表同查法），PT 项 = 结余 PT × rate。
pub struct LocalScoring {
    /// 五维 → 评分查表（新口径 = URA StatusToPoint 2501 档；空表降级本地 3399 档）
    pub table: Vec<i32>,
    /// 每 1 技能点折多少分（constants.json 的 pt_score_rate = 2.0）
    pub pt_rate: f32
}

impl LocalScoring {
    pub fn from_constants() -> Self {
        let c = GAMECONSTANTS
            .get()
            .expect("GAMECONSTANTS 未初始化，请先 init_global_with_config");
        // 与 Uma::status_score_ura 同源：优先 URA 表，空表回退本地 3399 档延拓表
        let table = if c.ura_status_to_point.is_empty() {
            c.five_status_final_score.clone()
        } else {
            c.ura_status_to_point.clone()
        };
        Self {
            table,
            pt_rate: c.pt_score_rate
        }
    }

    /// 旧口径计分器（本地 3399 档表 + 全部 PT×rate 不买技能）——仅供新旧口径对照打印。
    pub fn legacy_from_constants() -> Self {
        let c = GAMECONSTANTS
            .get()
            .expect("GAMECONSTANTS 未初始化，请先 init_global_with_config");
        Self {
            table: c.five_status_final_score.clone(),
            pt_rate: c.pt_score_rate
        }
    }

    /// 五维各自折算评分（min(值, 上限) 后查表，饱和）
    pub fn status_scores(&self, snap: &UmaSnapshot) -> [i32; 5] {
        let mut out = [0i32; 5];
        for i in 0..5 {
            out[i] = lookup_saturated(&self.table, snap.five_status[i].min(snap.five_status_limit[i]));
        }
        out
    }

    /// 属性评分小计
    pub fn status_total(&self, snap: &UmaSnapshot) -> i32 {
        self.status_scores(snap).iter().sum()
    }

    /// 技能点折分（与 UmaGame::calc_score 同式：先 floor 到整点，再乘率截断）
    pub fn pt_score(&self, pt: i32) -> i32 {
        (pt as f32 * self.pt_rate) as i32
    }

    /// 买入前的基础分（对照用）= 属性分 + 固有技能分 + 全部 PT 折算
    /// （等于未买技能时的 `UmaGame::calc_score`）。
    pub fn base_score(&self, snap: &UmaSnapshot) -> i32 {
        self.status_total(snap) + snap.skill_score + self.pt_score(snap.total_pt())
    }

    /// 模拟买入后的结算分：skill_score 加 ΣGrade、skill_pt 扣总花费后走同一套查表。
    /// 与 `UmaGame::calc_score`（终局已买入状态）逐位一致。
    pub fn score_after(&self, snap: &UmaSnapshot, plan: &BuyPlan) -> i32 {
        let mut after = snap.clone();
        after.skill_score += plan.grade_total;
        after.skill_pt -= plan.cost_total;
        self.status_total(&after) + after.skill_score + self.pt_score(after.total_pt())
    }
}

/// 技能数据库：skillDB + hintDB + URA 属性表，一次加载。
pub struct SkillDb {
    /// 全部技能（含固有/禁用，供展示与过滤）
    pub skills: Vec<SkillEntry>,
    by_id: HashMap<u32, usize>,
    /// 支援卡 card_id（idrank/10）→ 自带 hint 技能列表
    hints: HashMap<String, Vec<HintEntry>>,
    /// URA StatusToPoint 表（下标 = 属性值，值 = 评价点，0..2500）
    pub ura_status_table: Vec<i32>
}

/// 进程级共享 [`SkillDb`]（一次加载，GA 跑批 / rollout 终评共用）。
/// 加载失败缓存 `None`：终局买技能静默停用（core-only .so 无数据文件时保持可运行）。
static SHARED_DB: OnceLock<Option<SkillDb>> = OnceLock::new();

/// 取进程级共享 SkillDb；文件缺失/损坏时返回 None（调用方跳过买入，不报错）。
pub fn try_shared_db() -> Option<&'static SkillDb> {
    SHARED_DB
        .get_or_init(|| match SkillDb::load() {
            Ok(db) => Some(db),
            Err(e) => {
                log::warn!("skillDB/hintDB/URA表加载失败，终局买技能停用: {e:#}");
                None
            }
        })
        .as_ref()
}

impl SkillDb {
    /// 从 gamedata/ 下三个 JSON 加载（cwd 必须在 workspace 根，与 GAMEDATA 同约定）
    pub fn load() -> Result<Self> {
        let skills: Vec<SkillEntry> =
            load_json("gamedata/skillDB.json").context("加载 gamedata/skillDB.json 失败")?;
        let hints: HashMap<String, Vec<HintEntry>> =
            load_json("gamedata/hintDB.json").context("加载 gamedata/hintDB.json 失败")?;
        let ura_status_table: Vec<i32> = load_json("gamedata/ura_status_to_point.json")
            .context("加载 gamedata/ura_status_to_point.json 失败")?;
        let mut by_id = HashMap::with_capacity(skills.len());
        for (i, s) in skills.iter().enumerate() {
            by_id.insert(s.id, i);
        }
        Ok(Self {
            skills,
            by_id,
            hints,
            ura_status_table
        })
    }

    pub fn get(&self, id: u32) -> Option<&SkillEntry> {
        self.by_id.get(&id).map(|&i| &self.skills[i])
    }

    /// 给定卡组（6 个 idrank），收集支援卡自带 hint 技能集合。
    ///
    /// 返回 (技能 id → 折扣百分比)。同一技能多张卡带时取折扣最大那张（玩家视角：
    /// hint 等级叠到最高档）。近似映射：kind 0 → 10% off，kind 1 → 20% off。
    pub fn deck_hint_discounts(&self, deck: &[u32; 6]) -> BTreeMap<u32, i32> {
        let mut out: BTreeMap<u32, i32> = BTreeMap::new();
        for idrank in deck {
            let card_id = (idrank / 10).to_string();
            if let Some(list) = self.hints.get(&card_id) {
                for h in list {
                    let off = if h.kind >= 1 { 20 } else { 10 };
                    let e = out.entry(h.skill_id).or_insert(0);
                    if off > *e {
                        *e = off;
                    }
                }
            }
        }
        out
    }

    /// 卡组 hint 技能 id 集合（跨卡组对比用）
    pub fn deck_hint_ids(&self, deck: &[u32; 6]) -> HashSet<u32> {
        self.deck_hint_discounts(deck).keys().copied().collect()
    }

    /// 卡组 hint 技能明细：[(skill_id, 折扣%)]（跨卡组技能面对比用）。
    ///
    /// 注意：9081xxx 段技能在现有数据源里没有名字/价格/评分，这里只报 id 与折扣。
    pub fn deck_hint_skills(&self, deck: &[u32; 6]) -> Vec<(u32, i32)> {
        let mut v: Vec<(u32, i32)> = self.deck_hint_discounts(deck).into_iter().collect();
        v.sort_unstable();
        v
    }

    /// 两个技能面（skill_id 集合）的 Jaccard 相似度：交/并。空集对空集按 1.0。
    pub fn jaccard(a: &HashSet<u32>, b: &HashSet<u32>) -> f64 {
        if a.is_empty() && b.is_empty() {
            return 1.0;
        }
        let inter = a.intersection(b).count();
        let union = a.union(b).count();
        inter as f64 / union.max(1) as f64
    }

    /// URA 口径的五维评分（2501 档查表，饱和）
    pub fn ura_status_scores(&self, snap: &UmaSnapshot) -> [i32; 5] {
        let mut out = [0i32; 5];
        for i in 0..5 {
            out[i] = lookup_saturated(&self.ura_status_table, snap.five_status[i]);
        }
        out
    }

    /// 可购买技能清单：过滤出 pool 口径下「买得到」的技能，并算好实价。
    ///
    /// 返回 (技能引用, 实价)。实价 = 原价 × (100 − 折扣%) / 100，向下取整。
    pub fn buyable(&self, pool: SkillPool, deck: &[u32; 6]) -> Vec<(&SkillEntry, i32)> {
        let discount = match pool {
            SkillPool::All => BTreeMap::new(), // 全池不打折（近似假设，见模块头注释）
            SkillPool::DeckHint => self.deck_hint_discounts(deck)
        };
        self.skills
            .iter()
            .filter(|s| {
                s.disable_singlemode == 0
                    && s.cost > 0
                    && s.grade > 0
                    && (s.rarity == 1 || s.rarity == 2) // 只买普通/金技能
            })
            .filter(|s| match pool {
                SkillPool::All => true,
                SkillPool::DeckHint => discount.contains_key(&s.id)
            })
            .map(|s| {
                let off = discount.get(&s.id).copied().unwrap_or(0);
                let price = (s.cost as i64 * (100 - off as i64)) / 100;
                (s, price as i32)
            })
            .collect()
    }
}

/// 一条购买推荐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuyCandidate {
    /// 技能 id
    pub id: u32,
    /// 展示名
    pub name: String,
    /// 实价（含折扣，全池口径=原价）
    pub price: i32,
    /// 评分增量
    pub grade: i32,
    /// URA 口径性价比 = grade / price（越大越划算）
    pub value_ura: f64,
    /// 本仓库拉面杯口径净增分 = grade − price × pt_rate（负数=买了反而亏）
    pub delta_local: i32
}

/// 推荐购买序列（贪心）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct BuyPlan {
    /// 按顺序买入的技能
    pub buys: Vec<BuyCandidate>,
    /// 总花费（技能点）
    pub cost_total: i32,
    /// 总评分增量 = Σgrade
    pub grade_total: i32,
    /// 总净增分 = Σ(grade − price×pt_rate)
    pub delta_local_total: i32
}

/// 推荐排序策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyPolicy {
    /// URA 口径：按 grade/price 降序，花光 PT 为止（PT 留着不折分，买就赚）
    UraValue,
    /// 本仓库口径：按 grade−price×pt_rate 降序，只买净增 > 0 的（PT 本身值分）
    LocalDelta
}

/// 生成推荐购买序列。
///
/// - `policy=UraValue`：URA 的推荐逻辑，性价比降序贪心，预算耗尽为止。
/// - `policy=LocalDelta`：拉面杯结算口径，只买净增分为正的技能。
/// - `top_n` 目前只做语义占位：贪心买入始终以预算耗尽为界，
///   推荐表的展示条数由调用层（bin）自己截取。
pub fn recommend(
    db: &SkillDb,
    snap: &UmaSnapshot,
    pool: SkillPool,
    deck: &[u32; 6],
    _top_n: usize,
    policy: BuyPolicy
) -> BuyPlan {
    let pt_rate = GAMECONSTANTS.get().map(|c| c.pt_score_rate).unwrap_or(2.0);
    let mut cands: Vec<BuyCandidate> = db
        .buyable(pool, deck)
        .into_iter()
        .map(|(s, price)| {
            let grade = s.grade;
            BuyCandidate {
                id: s.id,
                name: s.display_name(),
                price,
                grade,
                value_ura: if price > 0 {
                    grade as f64 / price as f64
                } else {
                    0.0
                },
                delta_local: grade - (price as f32 * pt_rate) as i32
            }
        })
        .collect();
    match policy {
        BuyPolicy::UraValue => cands.sort_by(|a, b| b.value_ura.total_cmp(&a.value_ura)),
        BuyPolicy::LocalDelta => cands.sort_by(|a, b| b.delta_local.cmp(&a.delta_local))
    }

    let mut plan = BuyPlan::default();
    let mut budget = snap.total_pt();
    for c in &cands {
        if budget < c.price {
            continue;
        }
        if policy == BuyPolicy::LocalDelta && c.delta_local <= 0 {
            // 净增分非正的全跳过（序列已降序，后面只会更差）
            break;
        }
        budget -= c.price;
        plan.cost_total += c.price;
        plan.grade_total += c.grade;
        plan.delta_local_total += c.delta_local;
        plan.buys.push(c.clone());
    }
    plan
}

/// 一份评分摘要（单一口径）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoreSummary {
    /// 口径名（打印用）
    pub label: &'static str,
    /// 五维原始值
    pub five_status: [i32; 5],
    /// 五维各自折算评分
    pub five_status_scores: [i32; 5],
    /// 属性评分小计
    pub status_total: i32,
    /// 总技能点（获得的）
    pub pt_total: i32,
    /// 已使用技能点（模拟器口径恒 0：育成过程中不买技能）
    pub pt_used: i32,
    /// 剩余技能点
    pub pt_left: i32,
    /// 已学技能评分（固有 510）
    pub skill_learned: i32,
    /// 即将学习技能评分小计
    pub skill_planned: i32,
    /// 即将学习技能列表（名 / 价 / 分）
    pub planned_list: Vec<(String, i32, i32)>,
    /// 预测总分
    pub predicted: i32,
    /// 预测总分对应的评级名（如 US4）
    pub rank_name: String,
    /// 距下一评级还差多少分（已是最高档则 None）
    pub to_next_rank: Option<i32>
}

/// URA 口径摘要：`总分 = 属性评分(URA 表) + 已学技能分 + 即将学习技能分`。
///
/// 三者相加逐位一致（整数加法无浮点），与 URA 截图的「属性评分 + 已学 + 即将学 =
/// 预测总分」结构完全对应。
pub fn summary_ura(db: &SkillDb, snap: &UmaSnapshot, plan: &BuyPlan) -> ScoreSummary {
    let scores = db.ura_status_scores(snap);
    let status_total: i32 = scores.iter().sum();
    let skill_planned = plan.grade_total;
    let predicted = status_total + snap.skill_score + skill_planned;
    let (rank, gap) = rank_of(predicted);
    ScoreSummary {
        label: "URA口径(通用育成)",
        five_status: snap.five_status,
        five_status_scores: scores,
        status_total,
        pt_total: snap.total_pt(),
        pt_used: 0,
        pt_left: snap.total_pt() - plan.cost_total,
        skill_learned: snap.skill_score,
        skill_planned,
        planned_list: plan
            .buys
            .iter()
            .map(|b| (b.name.clone(), b.price, b.grade))
            .collect(),
        predicted,
        rank_name: rank,
        to_next_rank: gap
    }
}

/// 本仓库结算口径摘要（新口径 2026-09-17）：
/// `总分 = 属性分(URA表) + 固有技能分 + 买入技能分 + 结余PT×pt_rate`。
///
/// 与 `UmaGame::calc_score`（终局已买入状态）完全同式；`snap` 传买入前快照时，
/// 本函数给出「按 plan 买入后的预测终分」。
pub fn summary_local(local: &LocalScoring, snap: &UmaSnapshot, plan: &BuyPlan) -> ScoreSummary {
    let scores = local.status_scores(snap);
    let status_total: i32 = scores.iter().sum();
    let skill_planned = plan.grade_total;
    let pt_left = snap.total_pt() - plan.cost_total;
    let predicted = local.score_after(snap, plan);
    let (rank, gap) = rank_of(predicted);
    ScoreSummary {
        label: "本仓库口径(拉面杯RMJ)",
        five_status: snap.five_status,
        five_status_scores: scores,
        status_total,
        pt_total: snap.total_pt(),
        pt_used: 0,
        pt_left,
        skill_learned: snap.skill_score,
        skill_planned,
        planned_list: plan
            .buys
            .iter()
            .map(|b| (b.name.clone(), b.price, b.grade))
            .collect(),
        predicted,
        rank_name: rank,
        to_next_rank: gap
    }
}

/// 评级名 + 距下一档差分（查 constants.json 的 rank_scores/rank_names，与
/// `get_rank_name` 同源；US5 下界 69000 已与 URA 截图交叉验证）。
fn rank_of(score: i32) -> (String, Option<i32>) {
    let cons = GAMECONSTANTS
        .get()
        .expect("GAMECONSTANTS 未初始化，请先 init_global_with_config");
    let name = cons.get_rank_name(score);
    // 距下一档差分：找到第一个下界 > score 的档位
    let gap = cons
        .rank_scores
        .iter()
        .find(|&&t| t > score.max(0))
        .map(|&t| t - score);
    (name, gap)
}

/// 打印一份摘要（人读格式；JSON 输出走 bin 层的 serde）。
pub fn print_summary(s: &ScoreSummary) {
    let names = ["速度", "耐力", "力量", "根性", "智力"];
    println!("---- 评分摘要：{} ----", s.label);
    for i in 0..5 {
        println!(
            "  {:<4} 原始值 {:>5} → 属性评分 {:>6}",
            names[i], s.five_status[i], s.five_status_scores[i]
        );
    }
    println!("  属性评分小计          : {}", s.status_total);
    println!("  总技能点(获得)        : {}", s.pt_total);
    println!(
        "  已使用技能点          : {}（终局买技能账目；before 快照恒为 0）",
        s.pt_used
    );
    println!("  剩余技能点            : {}", s.pt_left);
    println!("  已学习技能评分        : {}", s.skill_learned);
    println!("  即将学习技能评分小计  : {}", s.skill_planned);
    for (name, price, grade) in &s.planned_list {
        println!("    - {name}  价格={price}  评分增量={grade}");
    }
    println!("  预测总分              : {}", s.predicted);
    match &s.to_next_rank {
        Some(gap) => println!(
            "  预测评级              : {}（距下一档还差 {gap} 分）",
            s.rank_name
        ),
        None => println!("  预测评级              : {}（已是最高档）", s.rank_name)
    }
}

/// 两口径的关键差异对照（一行一条，直接可贴报告）。
pub fn formula_compare(local: &LocalScoring, snap: &UmaSnapshot, db: &SkillDb) -> Vec<String> {
    let mut out = Vec::new();
    let ura: i32 = db.ura_status_scores(snap).iter().sum();
    let lo: i32 = local.status_scores(snap).iter().sum();
    out.push(format!(
        "属性分：URA表小计={ura} vs 本地3399档小计={lo}（新口径已采用 URA 表；0..1200 两表同源一致，1200+ 延拓不同）"
    ));
    out.push(format!(
        "技能点折分：URA 口径 PT 不直接计分（花掉才变技能分）；本仓库新口径 PT×{:.1} 折算（URA 截图对账），买技能扣减结余 PT",
        local.pt_rate
    ));
    out.push(
        "技能购买决策：URA 按 grade/price 排序（买必赚）；本仓库按 grade−price×2 排序，只买净增分>0 的，贪心买入后 ΣGrade 计入技能分"
            .to_string()
    );
    out
}

/// 验证一次「预测 Δ分 vs 实际 Δ分」（拉面杯口径）。
///
/// - 预测 Δ = Σgrade − Σprice × pt_rate（解析式）
/// - 实际 Δ = 买入后重算结算分 − 买入前结算分
/// 两者应逐位相等；偏差非 0 说明解析式与结算管道不一致（严重 bug）。
pub fn verify_delta(local: &LocalScoring, snap: &UmaSnapshot, plan: &BuyPlan) -> (i32, i32, i32) {
    let predicted = plan.grade_total - local.pt_score(plan.cost_total);
    let actual = local.score_after(snap, plan) - local.base_score(snap);
    (predicted, actual, actual - predicted)
}
