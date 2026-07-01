//! Regression test for Roblox storing `Tags` in the `SharedString` index.
//!
//! On 2026-06-30, Roblox Studio began serializing the `Tags` property as a
//! binary `SharedString` (type `0x1C`) instead of the usual `String` blob, to
//! deduplicate identical tag sets across instances. The deserializer must
//! decode such a `SharedString` payload back into a `Variant::Tags`.
//!
//! The serializer never emits `Tags` this way, so the fixture is hand-built as
//! a minimal uncompressed model.

use std::io::Write;

use rbx_dom_weak::types::{Tags, Variant};

use crate::{
    chunk::ChunkBuilder,
    core::{RbxWriteExt, RbxWriteInterleaved, FILE_MAGIC_HEADER, FILE_SIGNATURE, FILE_VERSION},
    from_reader,
    types::Type,
    CompressionType,
};

/// Builds a minimal, uncompressed binary model containing a single `Folder`
/// whose `Tags` property is serialized as a `SharedString` (type `0x1C`)
/// referencing the first `SSTR` entry, mirroring what Roblox now emits.
fn build_folder_with_shared_string_tags(tag_buffer: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();

    // File header.
    output.write_all(FILE_MAGIC_HEADER).unwrap();
    output.write_all(FILE_SIGNATURE).unwrap();
    output.write_le_u16(FILE_VERSION).unwrap();
    output.write_le_u32(1).unwrap(); // number of instance types
    output.write_le_u32(1).unwrap(); // number of instances
    output.write_all(&[0; 8]).unwrap(); // reserved

    // SSTR chunk: a single shared string holding the encoded Tags buffer.
    let mut sstr = ChunkBuilder::new(b"SSTR", CompressionType::None);
    sstr.write_le_u32(0).unwrap(); // SSTR version
    sstr.write_le_u32(1).unwrap(); // entry count
    sstr.write_all(&[0; 16]).unwrap(); // 16-byte hash (ignored on read)
    sstr.write_binary_string(tag_buffer).unwrap();
    sstr.dump(&mut output).unwrap();

    // INST chunk: one non-service Folder with referent 0.
    let mut inst = ChunkBuilder::new(b"INST", CompressionType::None);
    inst.write_le_u32(0).unwrap(); // type id
    inst.write_string("Folder").unwrap();
    inst.write_bool(false).unwrap(); // not a service
    inst.write_le_u32(1).unwrap(); // instance count
    inst.write_referent_array([0i32]).unwrap();
    inst.dump(&mut output).unwrap();

    // PROP chunk: Tags stored as a SharedString index array.
    let mut tags_prop = ChunkBuilder::new(b"PROP", CompressionType::None);
    tags_prop.write_le_u32(0).unwrap(); // type id
    tags_prop.write_string("Tags").unwrap();
    tags_prop.write_u8(Type::SharedString as u8).unwrap();
    tags_prop.write_interleaved_u32_array([0u32]).unwrap(); // -> SSTR entry 0
    tags_prop.dump(&mut output).unwrap();

    // PRNT chunk: the Folder is a root (null parent).
    let mut prnt = ChunkBuilder::new(b"PRNT", CompressionType::None);
    prnt.write_u8(0).unwrap(); // PRNT version
    prnt.write_le_u32(1).unwrap(); // count
    prnt.write_referent_array([0i32]).unwrap(); // object referents
    prnt.write_referent_array([-1i32]).unwrap(); // parent referents (null)
    prnt.dump(&mut output).unwrap();

    // END chunk.
    let mut end = ChunkBuilder::new(b"END\0", CompressionType::None);
    end.write_all(b"</roblox>").unwrap();
    end.dump(&mut output).unwrap();

    output
}

#[test]
fn tags_stored_as_shared_string() {
    let _ = env_logger::try_init();

    let mut tags = Tags::new();
    tags.push("Alpha");
    tags.push("Beta");
    let bytes = build_folder_with_shared_string_tags(tags.encode());

    let dom = from_reader(bytes.as_slice())
        .expect("a model with SharedString-encoded Tags should deserialize");

    let root = dom.root();
    let folder = dom
        .get_by_ref(root.children()[0])
        .expect("root should have a Folder child");

    assert_eq!(folder.class.as_str(), "Folder");

    let value = folder
        .properties
        .get(&rbx_dom_weak::ustr("Tags"))
        .expect("Folder should have a Tags property");

    assert_eq!(*value, Variant::Tags(tags));
}
