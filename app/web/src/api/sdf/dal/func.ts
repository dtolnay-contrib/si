export enum FuncBackendKind {
  Array = "Array",
  Boolean = "Boolean",
  Identity = "Identity",
  Integer = "Integer",
  JsQualification = "JsQualification",
  JsResourceSync = "JsResourceSync",
  JsCodeGeneration = "JsCodeGeneration",
  JsAttribute = "JsAttribute",
  Map = "Map",
  PropObject = "PropObject",
  String = "String",
  Unset = "Unset",
  Json = "Json",
  ValidateStringValue = "ValidateStringValue",
}

const CUSTOMIZABLE_FUNCS = [
  FuncBackendKind.JsQualification,
  FuncBackendKind.JsResourceSync,
  FuncBackendKind.JsAttribute,
  FuncBackendKind.JsCodeGeneration,
];

export const isCustomizableFuncKind = (f: FuncBackendKind) =>
  CUSTOMIZABLE_FUNCS.includes(f);

export interface Func {
  id: number;
  handler: string;
  kind: FuncBackendKind;
  name: string;
  description?: string;
  code?: string;
  isBuiltin: boolean;
}
