import type {Asset} from "../assets/types";
export const relationNames = {
  inspired_by:"灵感来自", generated_from:"生成自", derived_from:"衍生自",
  used_in:"用于", reference_of:"作为参考", variant_of:"变体", related_to:"相关",
} as const;
export type RelationType = keyof typeof relationNames;
export interface RelationInput {sourceAssetId:string;targetAssetId:string;relationType:RelationType}
export interface AssetRelation extends RelationInput {id:string;createdAt:string}
export interface AssetGraph {center:Asset;nodes:Asset[];relations:AssetRelation[];total:number;offset:number;limit:number}
