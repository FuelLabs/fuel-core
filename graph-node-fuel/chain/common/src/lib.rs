use std::collections::HashMap;
use std::fmt::Debug;

use anyhow::Error;
use protobuf::descriptor::field_descriptor_proto::Label;
use protobuf::descriptor::field_descriptor_proto::Type;
use protobuf::descriptor::DescriptorProto;
use protobuf::descriptor::FieldDescriptorProto;
use protobuf::descriptor::OneofDescriptorProto;
use protobuf::Message;
use protobuf::UnknownValueRef;
use std::convert::From;
use std::path::Path;

const REQUIRED_ID: u32 = 77001;

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub is_enum: bool,
    pub is_array: bool,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct PType {
    pub name: String,
    pub fields: Vec<Field>,
    pub descriptor: DescriptorProto,
}

impl PType {
    pub fn fields(&self) -> Option<String> {
        let mut v = Vec::new();
        if let Some(vv) = self.req_fields_as_string() {
            v.push(vv);
        }
        if let Some(vv) = self.enum_fields_as_string() {
            v.push(vv);
        }

        if v.is_empty() {
            None
        } else {
            Some(v.join(","))
        }
    }

    pub fn has_req_fields(&self) -> bool {
        self.fields.iter().any(|f| f.required)
    }

    pub fn req_fields_as_string(&self) -> Option<String> {
        if self.has_req_fields() {
            Some(format!(
                "__required__{{{}}}",
                self.fields
                    .iter()
                    .filter(|f| f.required)
                    .map(|f| format!("{}: {}", f.name, f.type_name))
                    .collect::<Vec<String>>()
                    .join(",")
            ))
        } else {
            None
        }
    }

    pub fn has_enum(&self) -> bool {
        self.fields.iter().any(|f| f.is_enum)
    }

    pub fn enum_fields_as_string(&self) -> Option<String> {
        if !self.has_enum() {
            return None;
        }

        Some(
            self.fields
                .iter()
                .filter(|f| f.is_enum)
                .map(|f| {
                    let pairs = f
                        .fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name, f.type_name))
                        .collect::<Vec<String>>()
                        .join(",");

                    format!("{}{{{}}}", f.name, pairs)
                })
                .collect::<Vec<String>>()
                .join(","),
        )
    }
}

impl From<&FieldDescriptorProto> for Field {
    fn from(fd: &FieldDescriptorProto) -> Self {
        let options = fd.options.unknown_fields();

        let type_name = if let Some(type_name) = fd.type_name.as_ref() {
            type_name.clone()
        } else if let Type::TYPE_BYTES = fd.type_() {
            "Vec<u8>".to_owned()
        } else {
            use heck::ToUpperCamelCase;
            fd.name().to_string().to_upper_camel_case()
        };

        Field {
            name: fd.name().to_owned(),
            type_name: type_name.rsplit('.').next().unwrap().to_owned(),
            required: options
                .iter()
                //(firehose.required) = true,  UnknownValueRef::Varint(0) => false, UnknownValueRef::Varint(1) => true
                .any(|f| f.0 == REQUIRED_ID && UnknownValueRef::Varint(1) == f.1),
            is_enum: false,
            is_array: Label::LABEL_REPEATED == fd.label(),
            fields: vec![],
        }
    }
}

impl From<&OneofDescriptorProto> for Field {
    fn from(fd: &OneofDescriptorProto) -> Self {
        Field {
            name: fd.name().to_owned(),
            type_name: "".to_owned(),
            required: false,
            is_enum: true,
            is_array: false,
            fields: vec![],
        }
    }
}

impl From<&DescriptorProto> for PType {
    fn from(dp: &DescriptorProto) -> Self {
        let mut fields = dp
            .oneof_decl
            .iter()
            .enumerate()
            .map(|(index, fd)| {
                let mut fld = Field::from(fd);

                fld.fields = dp
                    .field
                    .iter()
                    .filter(|fd| fd.oneof_index.is_some())
                    .filter(|fd| *fd.oneof_index.as_ref().unwrap() as usize == index)
                    .map(Field::from)
                    .collect::<Vec<Field>>();

                fld
            })
            .collect::<Vec<Field>>();

        fields.extend(
            dp.field
                .iter()
                .filter(|fd| fd.oneof_index.is_none())
                .map(Field::from)
                .collect::<Vec<Field>>(),
        );

        PType {
            name: dp.name().to_owned(),
            fields,
            descriptor: dp.clone(),
        }
    }
}

pub fn parse_proto_file<'a, P>(file_path: P) -> Result<HashMap<String, PType>, Error>
where
    P: 'a + AsRef<Path> + Debug,
{
    let dir = if let Some(p) = file_path.as_ref().parent() {
        p
    } else {
        return Err(anyhow::anyhow!(
            "Unable to derive parent path for {:?}",
            file_path
        ));
    };

    let fd = protobuf_parse::Parser::new()
        .include(dir)
        .input(&file_path)
        .file_descriptor_set()?;

    assert!(fd.file.len() == 1);
    assert!(fd.file[0].has_name());

    let file_name = file_path.as_ref().file_name().unwrap().to_str().unwrap();
    assert!(fd.file[0].name() == file_name);

    let ret_val = fd
        .file
        .iter() //should be just 1 file
        .flat_map(|f| f.message_type.iter())
        .map(|dp| (dp.name().to_owned(), PType::from(dp)))
        .collect::<HashMap<String, PType>>();

    Ok(ret_val)
}
