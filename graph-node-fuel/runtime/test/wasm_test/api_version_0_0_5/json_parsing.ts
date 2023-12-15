import { JSONValue, JSONValueKind, Bytes, Wrapped, Result } from './common/types'

enum IndexForAscTypeId {
  STRING = 0,
  ARRAY_BUFFER = 1,
  UINT8_ARRAY = 6,
  WRAPPED_BOOL = 28,
  WRAPPED_JSON_VALUE = 29,
  JSON_VALUE = 32,
  RESULT_JSON_VALUE_BOOL = 40,
}

export function id_of_type(type_id_index: IndexForAscTypeId): usize {
  switch (type_id_index) {
    case IndexForAscTypeId.STRING:
      return idof<string>();
    case IndexForAscTypeId.ARRAY_BUFFER:
      return idof<ArrayBuffer>();
    case IndexForAscTypeId.UINT8_ARRAY:
      return idof<Uint8Array>();
    case IndexForAscTypeId.WRAPPED_BOOL:
      return idof<Wrapped<bool>>();
    case IndexForAscTypeId.WRAPPED_JSON_VALUE:
      return idof<Wrapped<JSONValue>>();
    case IndexForAscTypeId.JSON_VALUE:
      return idof<JSONValue>();
    case IndexForAscTypeId.RESULT_JSON_VALUE_BOOL:
      return idof<Result<JSONValue, bool>>();
    default:
      return 0;
  }
}

export function allocate(n: usize): usize {
  return __alloc(n);
}

declare namespace json {
  function try_fromBytes(data: Bytes): Result<JSONValue, boolean>;
}

export function handleJsonError(data: Bytes): string {
  let result = json.try_fromBytes(data);
  if (result.isOk) {
    return "OK: " + result.value.toString() + ", ERROR: " + (result.isError ? "true" : "false");
  } else {
    return "ERROR: " + (result.error ? "true" : "false");
  }
}
