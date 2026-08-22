#!/usr/bin/env python3
import csv, glob, os, statistics
from collections import defaultdict

rows = defaultdict(list)
for path in glob.glob("matrix-results/**/matrix-result.csv", recursive=True):
    with open(path, newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            rows[row["variant"]].append({k: int(v) if k not in ("variant",) else v for k, v in row.items()})

def mean(xs): return statistics.fmean(xs) if xs else 0.0
def median(xs): return statistics.median(xs) if xs else 0.0
def pct(xs, p):
    if not xs: return 0.0
    xs = sorted(xs); pos = (len(xs)-1)*p; lo=int(pos); hi=min(lo+1,len(xs)-1); f=pos-lo
    return xs[lo]*(1-f)+xs[hi]*f

def esc(s): return s.replace("|", "\\|")

summary=[]
for variant, rs in rows.items():
    dp=[r["b_skill_pt"]-r["a_skill_pt"] for r in rs]
    ds=[r["b_status_score"]-r["a_status_score"] for r in rs]
    draw=[r["b_status_sum"]-r["a_status_sum"] for r in rs]
    dscore=[r["b_score"]-r["a_score"] for r in rs]
    both=sum(p>0 and s>=0 and sc>0 for p,s,sc in zip(dp,ds,dscore))/len(rs)*100
    pt_attr_loss=sum(p>0 and s<0 for p,s in zip(dp,ds))/len(rs)*100
    passed=mean(dp)>0 and mean(ds)>=0 and mean(dscore)>0
    summary.append(dict(variant=variant,n=len(rs),pt=mean([r["b_skill_pt"] for r in rs]),dpt=mean(dp),med=median(dp),p10=pct(dp,.1),p90=pct(dp,.9),ds=mean(ds),draw=mean(draw),dscore=mean(dscore),both=both,loss=pt_attr_loss,passed=passed))
summary.sort(key=lambda x:(x["passed"],x["dscore"],x["dpt"],x["ds"]), reverse=True)

out=os.environ.get("GITHUB_STEP_SUMMARY", "skill-pt-summary.md")
with open(out,"a",encoding="utf-8") as f:
    f.write("# 技能 PT 手写策略矩阵总结\n\n")
    f.write("> PT 指 `Uma.skill_pt`，不是 `ramen.scenario_pt`。A 为 PR 基准手写策略，B 为矩阵候选；所有结果使用相同 seed 配对。\n\n")
    if not summary:
        f.write("未找到矩阵结果。\n")
        raise SystemExit(1)
    best=summary[0]
    f.write(f"## 推荐结果\n\n**{esc(best['variant'])}** — 平均 ΔPT **{best['dpt']:+.1f}**，平均 Δ属性评分 **{best['ds']:+.1f}**，平均 Δ最终评分 **{best['dscore']:+.1f}**，验收 **{'✅' if best['passed'] else '❌'}**。\n\n")
    f.write("## 全部组合（按验收、最终评分、PT 排序）\n\n")
    f.write("|排名|组合|局数|B平均PT|ΔPT均值|ΔPT中位|P10/P90 ΔPT|Δ五维原值|Δ属性评分|Δ最终评分|PT↑且属性/评分不降|PT↑但属性降|验收|\n")
    f.write("|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---:|\n")
    for i,x in enumerate(summary,1):
        f.write(f"|{i}|`{esc(x['variant'])}`|{x['n']}|{x['pt']:.1f}|{x['dpt']:+.1f}|{x['med']:+.1f}|{x['p10']:+.0f}/{x['p90']:+.0f}|{x['draw']:+.1f}|{x['ds']:+.1f}|{x['dscore']:+.1f}|{x['both']:.1f}%|{x['loss']:.1f}%|{'✅' if x['passed'] else '❌'}|\n")
    f.write("\n## 验收规则\n\n- 平均 `Δskill_pt > 0`\n- 平均 `Δ五维属性评分 >= 0`\n- 平均 `Δcalc_score > 0`\n\nArtifact 仅供复核；主要结果就是本 Summary，无需下载 ZIP。\n")
