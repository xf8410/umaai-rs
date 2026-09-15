//! SSR 卡池模块（配卡基因搜索基础）。
//!
//! 数据来源：`gamedata/ssr_pool.json`（由 master.mdb 预生成，只含 master.mdb 中
//! rarity=3 且 command_id≠0 的卡）。原先 cardDB.json 中存在但 master.mdb 中没有的
//! 11 张卡（30307–30317）已恢复加入卡池，全量 SSR 共 296 张。
//!
//! 卡池按属性（速/耐/力/根/智）分组，每组内按 card_id 降序排列。
//! 配卡基因通过索引选择卡池中的卡；友人槽固定 card_id=30305 不参与搜索。

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Result, ensure};
use serde::Deserialize;

/// 属性下标（与 bench::TYPE_NAMES 对齐：0=速 1=耐 2=力 3=根 4=智）。
pub const ATTR_COUNT: usize = 5;

/// 属性英文名（与 bench::TYPE_NAMES 一致）。
pub const ATTR_NAMES: [&str; ATTR_COUNT] = ["speed", "stamina", "power", "guts", "wisdom"];

/// 属性中文名。
pub const ATTR_NAMES_ZH: [&str; ATTR_COUNT] = ["速", "耐", "力", "根", "智"];

/// 友人卡 card_id（焊死不参与搜索）。
pub const FRIEND_CARD_ID: u32 = 30305;

/// 友人卡满破 idrank。
pub const FRIEND_IDRANK: u32 = 303054;

/// 单张卡的池内记录。
#[derive(Debug, Clone, Deserialize)]
pub struct PoolCard {
    pub card_id: u32,
    pub idrank: u32,
    pub name: String,
    pub full_name: String,
    pub chara_id: u32,
    pub card_type: i32,
}

/// SSR 卡池（按属性分组）。
#[derive(Debug, Clone, Deserialize)]
pub struct SsrPoolData {
    pub source: String,
    pub total_cards: usize,
    pub pool: HashMap<String, Vec<PoolCard>>,
    pub excluded_from_card_db: Vec<u32>,
}

/// 全局卡池（惰性加载，进程级单例）。
static SSR_POOL: OnceLock<SsrPool> = OnceLock::new();

/// 运行时卡池（加载后不可变，提供按属性索引查询）。
#[derive(Debug, Clone)]
pub struct SsrPool {
    /// 按 ATTR_NAMES 下标排列的卡池；每池按 card_id 降序。
    pub pools: [Vec<PoolCard>; ATTR_COUNT],
    /// 被排除的 card_id（在 cardDB.json 中但不在 master.mdb 中）。
    pub excluded: Vec<u32>,
}

impl SsrPool {
    /// 从 gamedata/ssr_pool.json 加载。
    pub fn load() -> Result<Self> {
        let path = "gamedata/ssr_pool.json";
        let text = fs_err::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("读取卡池文件失败 {}: {e}", path))?;
        let data: SsrPoolData = serde_json::from_str(&text)?;

        let mut pools: [Vec<PoolCard>; ATTR_COUNT] = std::array::from_fn(|_| Vec::new());
        for (i, name) in ATTR_NAMES.iter().enumerate() {
            let mut cards = data
                .pool
                .get(*name)
                .cloned()
                .unwrap_or_default();
            cards.sort_by(|a, b| b.card_id.cmp(&a.card_id));
            pools[i] = cards;
        }

        let total: usize = pools.iter().map(|p| p.len()).sum();
        ensure!(
            total == data.total_cards,
            "卡池总数不一致: JSON 声明 {} 实际 {}",
            data.total_cards,
            total
        );

        Ok(Self {
            pools,
            excluded: data.excluded_from_card_db,
        })
    }

    /// 获取全局卡池（惰性加载）。
    pub fn global() -> &'static SsrPool {
        SSR_POOL.get_or_init(|| SsrPool::load().expect("SSR 卡池加载失败"))
    }

    /// 指定属性的池大小。
    pub fn pool_size(&self, attr: usize) -> usize {
        self.pools[attr].len()
    }

    /// 获取指定属性的第 idx 张卡（0-based，降序排列）。
    pub fn get_card(&self, attr: usize, idx: usize) -> Result<&PoolCard> {
        self.pools[attr]
            .get(idx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "属性 {}({}) 池大小 {} 但请求下标 {}",
                    attr,
                    ATTR_NAMES[attr],
                    self.pools[attr].len(),
                    idx
                )
            })
    }

    /// 获取指定属性的 idrank（满破 card_id*10+4）。
    pub fn get_idrank(&self, attr: usize, idx: usize) -> Result<u32> {
        Ok(self.get_card(attr, idx)?.idrank)
    }

    /// 属性池内是否包含指定 card_id。
    pub fn contains_card(&self, attr: usize, card_id: u32) -> bool {
        self.pools[attr].iter().any(|c| c.card_id == card_id)
    }

    /// 查找 card_id 在属性池中的下标。
    pub fn index_of(&self, attr: usize, card_id: u32) -> Option<usize> {
        self.pools[attr].iter().position(|c| c.card_id == card_id)
    }
}

/// 配卡选择：5 个属性各选一张代表卡的 idrank。
///
/// 对于需要多张同属性卡的 build，取该属性的选择卡 + 池内相邻卡。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CardSelection {
    /// 每属性的首选下标（0-based，降序池中的位置）。
    pub indices: [usize; ATTR_COUNT],
}

impl CardSelection {
    /// 默认选择：每属性取池内第一张（card_id 最大的 SSR）。
    pub fn default_top(_pool: &SsrPool) -> Self {
        Self {
            indices: [0; ATTR_COUNT],
        }
    }

    /// 根据选择生成卡组（[u32; 6]）。
    ///
    /// `counts[attr]` = 该属性需要的卡数（合计 = 5），友人卡追加在末尾。
    /// 若需要 k 张同属性卡，取该属性首选 + 池内后续 (k-1) 张（下标+1, +2, ...）。
    pub fn build_deck(&self, pool: &SsrPool, counts: &[usize; ATTR_COUNT]) -> Result<[u32; 6]> {
        let mut deck = Vec::with_capacity(6);
        for (attr, &count) in counts.iter().enumerate() {
            for j in 0..count {
                let idx = self.indices[attr] + j;
                let idrank = pool.get_idrank(attr, idx)?;
                deck.push(idrank);
            }
        }
        deck.push(FRIEND_IDRANK);
        ensure!(deck.len() == 6, "卡组必须恰好 6 张卡");
        // 同一副卡组 6 张卡 card_id 不得重复
        let mut ids: Vec<u32> = deck.iter().map(|&idrank| idrank / 10).collect();
        ids.sort();
        ids.dedup();
        ensure!(ids.len() == 6, "卡组 card_id 有重复: {:?}", deck);
        deck.try_into()
            .map_err(|_| anyhow::anyhow!("卡组长度异常"))
    }

    /// 变异：随机选一个属性，将下标换到池内另一个随机位置。
    pub fn mutate_one(&mut self, pool: &SsrPool, rng: &mut impl rand::Rng) {
        let attr = rng.random_range(0..ATTR_COUNT);
        let pool_size = pool.pool_size(attr);
        if pool_size <= 1 {
            return;
        }
        let new_idx = loop {
            let candidate = rng.random_range(0..pool_size);
            if candidate != self.indices[attr] {
                break candidate;
            }
        };
        self.indices[attr] = new_idx;
    }

    /// 均匀交叉：每个属性独立从父本 A 或 B 选取下标。
    pub fn crossover(a: &CardSelection, b: &CardSelection, rng: &mut impl rand::Rng) -> Self {
        let mut indices = [0usize; ATTR_COUNT];
        for i in 0..ATTR_COUNT {
            indices[i] = if rng.random::<bool>() {
                b.indices[i]
            } else {
                a.indices[i]
            };
        }
        Self { indices }
    }

    /// 卡选择 → 缓存键分量（SipHash）。
    pub fn hash_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.indices.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use crate::utils::{Checks, get_workspace_root};

    fn bootstrap() -> Result<()> {
        let workspace_root = get_workspace_root()?;
        std::env::set_current_dir(&workspace_root)?;
        let _ = crate::gamedata::init_global();
        Ok(())
    }

    /// 卡池加载：每属性非空、总数正确、降序排列。
    #[test]
    fn card_pool_load_and_structure() -> Result<()> {
        bootstrap()?;
        let pool = SsrPool::load()?;
        let mut c = Checks::new();

        let total: usize = pool.pools.iter().map(|p| p.len()).sum();
        println!("卡池总数: {} (排除: {:?})", total, pool.excluded);
        c.check(total > 200, &format!("卡池总数 > 200（实际 {total}）"));

        for (i, name) in ATTR_NAMES.iter().enumerate() {
            let size = pool.pool_size(i);
            c.check(size > 20, &format!("{name} 池大小 {size} > 20"));
            // 降序检查
            let sorted = pool.pools[i]
                .windows(2)
                .all(|w| w[0].card_id > w[1].card_id);
            c.check(sorted, &format!("{name} 池按 card_id 降序"));
        }

        // 友人卡不在任何属性池中
        for (i, name) in ATTR_NAMES.iter().enumerate() {
            c.check(
                !pool.contains_card(i, FRIEND_CARD_ID),
                &format!("{name} 池不含友人卡 {FRIEND_CARD_ID}")
            );
        }

        // 排除列表已清空（原 30307-30317 的 11 张卡已恢复加入卡池）
        c.check(
            pool.excluded.is_empty(),
            &format!("排除列表为空（实际 {:?}）", pool.excluded)
        );

        // 各属性具体数量（含恢复的 11 张：速+3 耐+2 力+1 根+3 智+2；
        // 剔除美浦波旁本体卡 5 张——游戏规则马娘不能装备自己本体的支援卡）
        let expected_sizes = [71, 54, 56, 59, 51]; // speed, stamina, power, guts, wisdom
        for (i, &expected) in expected_sizes.iter().enumerate() {
            c.check(
                pool.pool_size(i) == expected,
                &format!("{} 池大小 {} = 期望 {}", ATTR_NAMES[i], pool.pool_size(i), expected)
            );
        }

        c.finish()
    }

    /// 默认卡组：每属性取第一张 + 友人 → 与 bench::CardPickOpts::default() 同源。
    #[test]
    fn card_selection_default_deck() -> Result<()> {
        bootstrap()?;
        let pool = SsrPool::load()?;
        let sel = CardSelection::default_top(&pool);
        let counts = [3usize, 1, 0, 0, 1]; // speed build
        let deck = sel.build_deck(&pool, &counts)?;

        let mut c = Checks::new();
        println!("默认速主卡组: {:?}", deck);
        c.check(deck.len() == 6, "卡组 6 张");
        c.check(deck[5] == FRIEND_IDRANK, "末位 = 友人 303054");
        // 前三张都是速度池的卡
        for j in 0..3 {
            c.check(
                pool.contains_card(0, deck[j] / 10),
                &format!("slot {j} = {} 在速度池中", deck[j])
            );
        }
        // 无重复
        let mut ids: Vec<u32> = deck.iter().map(|&id| id / 10).collect();
        let orig_len = ids.len();
        ids.sort();
        ids.dedup();
        c.check(ids.len() == orig_len, "卡组无重复 card_id");
        c.finish()
    }

    /// 不同选择 → 不同卡组 → 不同哈希。
    #[test]
    fn card_selection_distinct_hash() -> Result<()> {
        bootstrap()?;
        let pool = SsrPool::load()?;
        let sel_a = CardSelection::default_top(&pool);
        let mut sel_b = sel_a;
        sel_b.indices[0] = 1; // 速度选第二张

        let mut c = Checks::new();
        c.check(sel_a.hash_key() != sel_b.hash_key(), "不同选择 → 不同哈希");

        let counts = [3usize, 1, 0, 0, 1];
        let deck_a = sel_a.build_deck(&pool, &counts)?;
        let deck_b = sel_b.build_deck(&pool, &counts)?;
        c.check(deck_a != deck_b, "不同选择 → 不同卡组");
        c.finish()
    }

    /// 交叉与变异操作。
    #[test]
    fn card_selection_crossover_and_mutate() -> Result<()> {
        bootstrap()?;
        let pool = SsrPool::load()?;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let a = CardSelection { indices: [0; ATTR_COUNT] };
        let b = CardSelection { indices: [1; ATTR_COUNT] };

        let child = CardSelection::crossover(&a, &b, &mut rng);
        println!("交叉结果: {:?}", child.indices);

        let before = child.indices;
        let mut mutated = child;
        mutated.mutate_one(&pool, &mut rng);
        println!("变异结果: {:?}", mutated.indices);

        let mut c = Checks::new();
        // 交叉每位来自 a 或 b
        for i in 0..ATTR_COUNT {
            c.check(
                mutated.indices[i] == 0 || mutated.indices[i] == 1 || mutated.indices[i] != before[i],
                &format!("属性 {i}: 交叉/变异后值合法")
            );
        }
        c.finish()
    }
}
