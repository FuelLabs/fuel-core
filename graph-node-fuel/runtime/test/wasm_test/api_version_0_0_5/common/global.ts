__alloc(0);

import { BigDecimal, TypedMapEntry, Entity, TypedMap, Result, Wrapped, JSONValue, Value, Token } from './types'

export enum TypeId {
  String = 0,
  ArrayBuffer = 1,
  Int8Array = 2,
  Int16Array = 3,
  Int32Array = 4,
  Int64Array = 5,
  Uint8Array = 6,
  Uint16Array = 7,
  Uint32Array = 8,
  Uint64Array = 9,
  Float32Array = 10,
  Float64Array = 11,
  BigDecimal = 12,
  ArrayBool = 13,
  ArrayUint8Array = 14,
  ArrayEthereumValue = 15,
  ArrayStoreValue = 16,
  ArrayJsonValue = 17,
  ArrayString = 18,
  ArrayEventParam = 19,
  ArrayTypedMapEntryStringJsonValue = 20,
  ArrayTypedMapEntryStringStoreValue = 21,
  SmartContractCall = 22,
  EventParam = 23,
  // EthereumTransaction = 24,
  // EthereumBlock = 25,
  // EthereumCall = 26,
  WrappedTypedMapStringJsonValue = 27,
  WrappedBool = 28,
  WrappedJsonValue = 29,
  EthereumValue = 30,
  StoreValue = 31,
  JsonValue = 32,
  // EthereumEvent = 33,
  TypedMapEntryStringStoreValue = 34,
  TypedMapEntryStringJsonValue = 35,
  TypedMapStringStoreValue = 36,
  TypedMapStringJsonValue = 37,
  TypedMapStringTypedMapStringJsonValue = 38,
  ResultTypedMapStringJsonValueBool = 39,
  ResultJsonValueBool = 40,
  ArrayU8 = 41,
  ArrayU16 = 42,
  ArrayU32 = 43,
  ArrayU64 = 44,
  ArrayI8 = 45,
  ArrayI16 = 46,
  ArrayI32 = 47,
  ArrayI64 = 48,
  ArrayF32 = 49,
  ArrayF64 = 50,
  ArrayBigDecimal = 51,
}

export function id_of_type(typeId: TypeId): usize {
  switch (typeId) {
    case TypeId.String:
      return idof<string>()
    case TypeId.ArrayBuffer:
      return idof<ArrayBuffer>()
    case TypeId.Int8Array:
      return idof<Int8Array>()
    case TypeId.Int16Array:
      return idof<Int16Array>()
    case TypeId.Int32Array:
      return idof<Int32Array>()
    case TypeId.Int64Array:
      return idof<Int64Array>()
    case TypeId.Uint8Array:
      return idof<Uint8Array>()
    case TypeId.Uint16Array:
      return idof<Uint16Array>()
    case TypeId.Uint32Array:
      return idof<Uint32Array>()
    case TypeId.Uint64Array:
      return idof<Uint64Array>()
    case TypeId.Float32Array:
      return idof<Float32Array>()
    case TypeId.Float64Array:
      return idof<Float64Array>()
    case TypeId.BigDecimal:
      return idof<BigDecimal>()
    case TypeId.ArrayBool:
      return idof<Array<bool>>()
    case TypeId.ArrayUint8Array:
      return idof<Array<Uint8Array>>()
    case TypeId.ArrayEthereumValue:
      return idof<Array<Token>>()
    case TypeId.ArrayStoreValue:
      return idof<Array<Value>>()
    case TypeId.ArrayJsonValue:
      return idof<Array<JSONValue>>()
    case TypeId.ArrayString:
      return idof<Array<string>>()
    // case TypeId.ArrayEventParam:
    //   return idof<Array<ethereum.EventParam>>()
    case TypeId.ArrayTypedMapEntryStringJsonValue:
      return idof<Array<TypedMapEntry<string, JSONValue>>>()
    case TypeId.ArrayTypedMapEntryStringStoreValue:
      return idof<Array<Entity>>()
    case TypeId.WrappedTypedMapStringJsonValue:
      return idof<Wrapped<TypedMapEntry<string, JSONValue>>>()
    case TypeId.WrappedBool:
      return idof<Wrapped<boolean>>()
    case TypeId.WrappedJsonValue:
      return idof<Wrapped<JSONValue>>()
    // case TypeId.SmartContractCall:
    //   return idof<ethereum.SmartContractCall>()
    // case TypeId.EventParam:
    //   return idof<ethereum.EventParam>()
    // case TypeId.EthereumTransaction:
    //   return idof<ethereum.Transaction>()
    // case TypeId.EthereumBlock:
    //   return idof<ethereum.Block>()
    // case TypeId.EthereumCall:
    //   return idof<ethereum.Call>()
    case TypeId.EthereumValue:
      return idof<Token>()
    case TypeId.StoreValue:
      return idof<Value>()
    case TypeId.JsonValue:
      return idof<JSONValue>()
    // case TypeId.EthereumEvent:
    //   return idof<ethereum.Event>()
    case TypeId.TypedMapEntryStringStoreValue:
      return idof<Entity>()
    case TypeId.TypedMapEntryStringJsonValue:
      return idof<TypedMap<string, JSONValue>>()
    case TypeId.TypedMapStringStoreValue:
      return idof<TypedMap<string, Value>>()
    case TypeId.TypedMapStringJsonValue:
      return idof<TypedMap<string, JSONValue>>()
    case TypeId.TypedMapStringTypedMapStringJsonValue:
      return idof<TypedMap<string, TypedMap<string, JSONValue>>>()
    case TypeId.ResultTypedMapStringJsonValueBool:
      return idof<Result<TypedMap<string, JSONValue>, boolean>>()
    case TypeId.ResultJsonValueBool:
      return idof<Result<JSONValue, boolean>>()
    case TypeId.ArrayU8:
      return idof<Array<u8>>()
    case TypeId.ArrayU16:
      return idof<Array<u16>>()
    case TypeId.ArrayU32:
      return idof<Array<u32>>()
    case TypeId.ArrayU64:
      return idof<Array<u64>>()
    case TypeId.ArrayI8:
      return idof<Array<i8>>()
    case TypeId.ArrayI16:
      return idof<Array<i16>>()
    case TypeId.ArrayI32:
      return idof<Array<i32>>()
    case TypeId.ArrayI64:
      return idof<Array<i64>>()
    case TypeId.ArrayF32:
      return idof<Array<f32>>()
    case TypeId.ArrayF64:
      return idof<Array<f64>>()
    case TypeId.ArrayBigDecimal:
      return idof<Array<BigDecimal>>()
    default:
      return 0
  }
}

export function allocate(size: usize): usize {
  return __alloc(size)
}
