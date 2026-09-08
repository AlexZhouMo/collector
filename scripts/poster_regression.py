#!/usr/bin/env python3
"""海报抓取回归校验工具。

遍历库中所有已从 TMDB 抓到海报的条目（cover_path 含 tmdb_），用当前抓取规则
重新搜索 TMDB，对比命中的 poster_path 是否与基准一致。用于每次修改抓取规则后，
确认没有把原本抓对的条目改坏。

本脚本严格复刻 Rust 抓取规则，对应关系见下方注释：
- parse_query / clean_title / subtitle_main / parse_season → src-tauri/src/poster/parse.rs
- 搜索顺序（完整名→动漫fallback tv→alt_name→alt_name动漫fallback tv）→ src-tauri/src/lib.rs fetch_cover
- 季海报回退 → src-tauri/src/lib.rs + src-tauri/src/poster/tmdb.rs season_poster

用法：
  python3 scripts/poster_regression.py --build-baseline   # 首次：建立基准
  python3 scripts/poster_regression.py                     # 校验：与基准对比
"""
import os
import sys
import re
import json
import time
import sqlite3
import urllib.parse
import urllib.request

DB = os.path.expanduser(
    "~/Library/Application Support/com.zhoumo.collector/collector.sqlite"
)
BASELINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "poster_baseline.json")
API_BASE = "https://api.tmdb.org/3"  # 与 Rust 一致：主域名被墙，用备用域名
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0"
SLEEP = 0.1


# ---- 复刻 parse.rs::clean_title ----
def clean_title(title):
    """剥离 [YYYY]. 或裸 YYYY. 前缀，返回 (name, year|None)。"""
    # 形式一：[YYYY] 前缀
    if title.startswith("["):
        close = title.find("]")
        if close != -1:
            inner = title[1:close]
            if len(inner) == 4 and inner.isdigit():
                after = title[close + 1:]
                if after.startswith("."):
                    after = after[1:]
                return after.strip(), int(inner)
    # 形式二：裸 YYYY. 前缀（前 4 字符数字 + '.'）
    t = title.strip()
    if len(t) > 5 and t[:4].isdigit() and t[4] == ".":
        return t[5:].strip(), int(t[:4])
    return t, None


# ---- 复刻 parse.rs::subtitle_main ----
def subtitle_main(name):
    """name 含中文/英文冒号 → 冒号前主名；否则 None。"""
    idx = name.find("：")
    if idx == -1:
        idx = name.find(":")
    if idx == -1:
        return None
    main = name[:idx].strip()
    if not main or main == name:
        return None
    return main


# ---- 复刻 parse.rs::parse_season ----
def parse_season(seg):
    """匹配「第N季」开头，其后可跟副标题。返回季号或 None。"""
    if not seg.startswith("第"):
        return None
    rest = seg[1:]
    idx = rest.find("季")
    if idx == -1:
        return None
    num = rest[:idx]
    if not num or not num.isdigit():
        return None
    return int(num)


# ---- 复刻 parse.rs::parse_query ----
def parse_query(category, category_path, title):
    """返回 dict{name, kind('movie'|'tv'), season, year, is_anime, alt_name}。"""
    segs = [s for s in category_path.split("/") if s]
    last = segs[-1] if segs else title
    if category == "剧集":
        season = parse_season(last)
        if season is not None:
            name = segs[-2] if len(segs) >= 2 else last
            return dict(name=name, kind="tv", season=season, year=None, is_anime=False, alt_name=None)
        return dict(name=last, kind="tv", season=None, year=None, is_anime=False, alt_name=None)
    name, year = clean_title(title)
    alt = subtitle_main(name)
    if category == "动漫":
        return dict(name=name, kind="movie", season=None, year=year, is_anime=True, alt_name=alt)
    return dict(name=name, kind="movie", season=None, year=year, is_anime=False, alt_name=alt)


# ---- 复刻 tmdb.rs::search ----
def _search(key, name, kind, year):
    q = urllib.parse.quote(name)
    url = f"{API_BASE}/search/{'movie' if kind=='movie' else 'tv'}?api_key={key}&language=zh-CN&query={q}"
    if year:
        url += f"&year={year}" if kind == "movie" else f"&first_air_date_year={year}"
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            d = json.load(r)
    except Exception:
        return None
    for it in d.get("results", []):
        if it.get("poster_path"):
            return {"id": it["id"], "poster_path": it["poster_path"]}
    return None


def _season_poster(key, tv_id, season):
    url = f"{API_BASE}/tv/{tv_id}/season/{season}?api_key={key}&language=zh-CN"
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            d = json.load(r)
    except Exception:
        return None
    return d.get("poster_path")


# ---- 复刻 lib.rs::fetch_cover 的搜索顺序，返回最终 poster_path ----
def resolve_poster_path(key, q):
    hit = _search(key, q["name"], q["kind"], q["year"])
    time.sleep(SLEEP)
    if hit is None and q["is_anime"]:
        hit = _search(key, q["name"], "tv", q["year"]); time.sleep(SLEEP)
    if hit is None and q["alt_name"]:
        hit = _search(key, q["alt_name"], q["kind"], q["year"]); time.sleep(SLEEP)
        if hit is None and q["is_anime"]:
            hit = _search(key, q["alt_name"], "tv", q["year"]); time.sleep(SLEEP)
    if hit is None:
        return None
    # 剧集季海报回退
    if q["kind"] == "tv" and q["season"] is not None:
        sp = _season_poster(key, hit["id"], q["season"]); time.sleep(SLEEP)
        if sp:
            return sp
    return hit["poster_path"]


def load_items(conn):
    return conn.execute(
        """SELECT id, category, category_path, title FROM media_item
           WHERE kind='video' AND cover_path LIKE '%tmdb_%' ORDER BY id"""
    ).fetchall()


def get_key(conn):
    row = conn.execute("SELECT value FROM settings WHERE key='tmdb_api_key'").fetchone()
    return row[0] if row else None


def main():
    build = "--build-baseline" in sys.argv
    conn = sqlite3.connect(DB)
    key = get_key(conn)
    if not key:
        print("错误：settings 表无 tmdb_api_key")
        sys.exit(1)
    items = load_items(conn)
    total = len(items)
    print(f"已抓条目：{total}")

    if build:
        # 断点续跑：已存在的基准文件中已完成的条目跳过，中断后重跑不必从头。
        baseline = {}
        if os.path.exists(BASELINE):
            try:
                with open(BASELINE, encoding="utf-8") as f:
                    baseline = json.load(f)
                print(f"已有基准 {len(baseline)} 条，续跑剩余部分（删除 {BASELINE} 可强制重建）")
            except Exception:
                baseline = {}
        done_before = len(baseline)
        for i, (id_, cat, cpath, title) in enumerate(items, 1):
            if str(id_) in baseline:
                continue  # 已完成，跳过
            q = parse_query(cat, cpath, title)
            pp = resolve_poster_path(key, q)
            baseline[str(id_)] = {"title": title, "poster_path": pp}
            # 每条实时进度（flush，后台重定向也能实时看到）
            print(f"  [{i}/{total}] {title} → {'命中' if pp else '无'}", flush=True)
            # 每 20 条增量写盘，中断也不丢进度
            if len(baseline) % 20 == 0:
                with open(BASELINE, "w", encoding="utf-8") as f:
                    json.dump(baseline, f, ensure_ascii=False, indent=0)
        with open(BASELINE, "w", encoding="utf-8") as f:
            json.dump(baseline, f, ensure_ascii=False, indent=0)
        got = sum(1 for v in baseline.values() if v["poster_path"])
        print(f"\n基准已写入 {BASELINE}：{len(baseline)} 条"
              f"（本次新增 {len(baseline)-done_before}，命中 poster_path {got} 条）", flush=True)
        return

    # 校验模式
    if not os.path.exists(BASELINE):
        print(f"错误：基准文件不存在，请先运行 --build-baseline")
        sys.exit(1)
    with open(BASELINE, encoding="utf-8") as f:
        baseline = json.load(f)

    same = changed = lost = 0
    changed_list = []
    lost_list = []
    items_by_id = {str(id_): (cat, cpath, title) for id_, cat, cpath, title in items}

    for i, (id_str, base) in enumerate(baseline.items(), 1):
        if id_str not in items_by_id:
            continue  # 该条目已从库中删除，跳过
        cat, cpath, title = items_by_id[id_str]
        q = parse_query(cat, cpath, title)
        new_pp = resolve_poster_path(key, q)
        old_pp = base["poster_path"]
        if new_pp == old_pp:
            same += 1
        elif new_pp is None:
            lost += 1
            lost_list.append((id_str, base["title"], old_pp))
        else:
            changed += 1
            changed_list.append((id_str, base["title"], old_pp, new_pp))
        if i % 50 == 0:
            print(f"  校验 {i}/{len(baseline)}")

    print(f"\n===== 校验结果：一致 {same} / 变化 {changed} / 丢失 {lost} =====")
    if changed_list:
        print(f"\n-- 变化（poster_path 不同，需警惕回归）--")
        for id_, title, old, new in changed_list:
            print(f"  [{id_}] {title}: {old} → {new}")
    if lost_list:
        print(f"\n-- 丢失（原有基准现在搜不到）--")
        for id_, title, old in lost_list:
            print(f"  [{id_}] {title}: 基准 {old} → 现无命中")
    if changed == 0 and lost == 0:
        print("\n✓ 规则更新未影响已抓结果。")


if __name__ == "__main__":
    main()
