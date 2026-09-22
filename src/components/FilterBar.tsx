import { Check, ChevronDown, SlidersHorizontal, X } from "lucide-react";
import type { FilterOptions, SearchRequest, SortMode } from "../types";
import { HoverDismissDetails } from "./HoverDismissDetails";

type MultiKey = "tags" | "dccTools" | "versions" | "formats" | "licenses";

interface Props {
  request: SearchRequest;
  options: FilterOptions;
  onChange: (patch: Partial<SearchRequest>) => void;
}

const groups: { key: MultiKey; label: string }[] = [
  { key: "tags", label: "标签" }, { key: "dccTools", label: "软件" },
  { key: "versions", label: "版本" }, { key: "formats", label: "格式" }, { key: "licenses", label: "许可" }
];

const sortOptions: { value: SortMode; label: string }[] = [
  { value: "relevance", label: "相关度" }, { value: "updated", label: "最近更新" },
  { value: "name", label: "名称" }, { value: "created", label: "创建时间" },
  { value: "recent", label: "最近查看" }, { value: "favorite", label: "收藏优先" }
];

export function FilterBar({ request, options, onChange }: Props) {
  const active = groups.reduce((n, group) => n + request[group.key].length, 0);
  const currentSort = sortOptions.find(option => option.value === request.sort)?.label || "最近更新";
  const toggle = (key: MultiKey, value: string) => {
    const values = request[key];
    onChange({ [key]: values.includes(value) ? values.filter(v => v !== value) : [...values, value] });
  };

  return <div className="filter-bar">
    <span className="filter-label"><SlidersHorizontal size={15} /> 筛选</span>
    {groups.map(group => <HoverDismissDetails className="filter-menu" key={group.key}>
      <summary className={request[group.key].length ? "active" : ""}>{group.label}{request[group.key].length ? ` ${request[group.key].length}` : ""}<ChevronDown size={13} /></summary>
      <div className="filter-popover">
        {options[group.key].length === 0 && <div className="empty-mini">暂无选项</div>}
        {options[group.key].map(value => <button key={value} onClick={() => toggle(group.key, value)}>
          <span className={`check-box ${request[group.key].includes(value) ? "checked" : ""}`}>{request[group.key].includes(value) && <Check size={12} />}</span>{value}
        </button>)}
      </div>
    </HoverDismissDetails>)}
    {active > 0 && <button className="clear-filters" onClick={() => onChange({ tags: [], dccTools: [], versions: [], formats: [], licenses: [] })}><X size={13} />清除 {active}</button>}
    <div className="sort-select"><span>排序</span><HoverDismissDetails className="sort-menu">
      <summary>{currentSort}<ChevronDown size={13} /></summary>
      <div className="sort-popover">
        {sortOptions.map(option => <button className={request.sort === option.value ? "active" : ""} key={option.value} onClick={() => onChange({ sort: option.value as SortMode })}>{option.label}</button>)}
      </div>
    </HoverDismissDetails></div>
  </div>;
}
