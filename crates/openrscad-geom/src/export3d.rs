//! Deterministic native 3D serialization over exact attributed geometry.

use super::mesh::{cross, normalize, package_3mf, sub};
use super::structured::{
    AttributedMesh, AttributionStatus, MeshSelection, StructuredMesh, SurfaceAttribution,
};
use super::{DisplayMode, GeomError, Mesh};
use openrscad_ir::{ProvenanceFrame, SourceId, SourceSpan};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(feature = "benchmark-profile")]
use web_time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat3D {
    Stl,
    Off,
    Obj,
    ThreeMf,
    Amf,
    Glb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSystem {
    YUp,
    ZUp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Export3DOptions {
    pub include_edges: bool,
    pub source_unit_to_meters: f64,
    pub coordinate_system: CoordinateSystem,
    pub source_keys: Vec<String>,
}

impl Default for Export3DOptions {
    fn default() -> Self {
        Self {
            include_edges: false,
            source_unit_to_meters: 0.001,
            coordinate_system: CoordinateSystem::YUp,
            source_keys: vec!["<main>".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Export3DArtifact {
    pub bytes: Vec<u8>,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub volume: f64,
    pub surface_area: f64,
}

pub fn export_3d(
    structured: &StructuredMesh,
    format: ExportFormat3D,
    options: &Export3DOptions,
) -> Result<Export3DArtifact, GeomError> {
    let mesh = &structured.aggregate;
    let bytes = match format {
        ExportFormat3D::Glb => serialize_glb(structured, options)?,
        ExportFormat3D::ThreeMf => serialize_3mf(structured, options)?,
        _ => {
            return Err(GeomError::Invariant(
                "authored-scene export only supports GLB and 3MF".to_string(),
            ));
        }
    };
    Ok(Export3DArtifact {
        bytes,
        triangle_count: mesh.tris.len(),
        vertex_count: mesh.verts.len(),
        volume: mesh.volume(),
        surface_area: mesh.surface_area(),
    })
}

fn rgba_bytes(rgba: [f32; 4]) -> [u8; 4] {
    rgba.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

#[derive(Clone, Copy)]
struct MeshSource<'a> {
    exact: &'a AttributedMesh,
    attributions: &'a [SurfaceAttribution],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialKey {
    mode: u8,
    rgba: [u8; 4],
}

impl MaterialKey {
    fn from_surface(surface: &SurfaceAttribution) -> Self {
        Self {
            mode: match surface.mode {
                DisplayMode::Solid => 0,
                DisplayMode::Highlight => 1,
                DisplayMode::Background => 2,
            },
            rgba: rgba_bytes(surface.rgba),
        }
    }

    fn display_mode(self) -> &'static str {
        match self.mode {
            1 => "highlight",
            2 => "background",
            _ => "solid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SemanticFrameKey {
    call_source: SourceId,
    call_start: u32,
    call_end: u32,
    definition: Option<(SourceId, u32, u32)>,
    module_name: String,
}

impl From<&ProvenanceFrame> for SemanticFrameKey {
    fn from(frame: &ProvenanceFrame) -> Self {
        Self {
            call_source: frame.call_site.source_id,
            call_start: frame.call_site.start,
            call_end: frame.call_site.end,
            definition: frame
                .definition_site
                .as_ref()
                .map(|span| (span.source_id, span.start, span.end)),
            module_name: frame
                .module_name
                .clone()
                .expect("semantic frames have authored module names"),
        }
    }
}

#[derive(Default)]
struct SourceTriangles {
    by_material: BTreeMap<MaterialKey, Vec<u32>>,
    all: Vec<u32>,
}

struct ComponentBuilder {
    key: SemanticFrameKey,
    frame: ProvenanceFrame,
    parent: Option<usize>,
    children: Vec<usize>,
    sources: BTreeMap<usize, SourceTriangles>,
    attribution: AttributionStatus,
    contributors: Vec<Vec<ProvenanceFrame>>,
}

struct GlbPrimitive<'a> {
    source: MeshSource<'a>,
    selection: MeshSelection,
    material: MaterialKey,
}

struct GlbEdgeGroup<'a> {
    source: MeshSource<'a>,
    selection: MeshSelection,
}

struct GlbComponent<'a> {
    frame: Option<ProvenanceFrame>,
    children: Vec<usize>,
    name: String,
    primitives: Vec<GlbPrimitive<'a>>,
    edge_groups: Vec<GlbEdgeGroup<'a>>,
    attribution: AttributionStatus,
    contributors: Vec<Vec<ProvenanceFrame>>,
}

struct CompiledGlb<'a> {
    components: Vec<GlbComponent<'a>>,
    roots: Vec<usize>,
}

fn material_rgba_from_key(material: MaterialKey) -> [u8; 4] {
    material.rgba
}

fn humanize_identifier(identifier: &str) -> String {
    identifier
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            first.to_uppercase().chain(characters).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn semantic_sibling_names(builders: &[ComponentBuilder], roots: &[usize]) -> Vec<String> {
    let mut names = vec![String::new(); builders.len()];
    let sibling_groups =
        std::iter::once(roots).chain(builders.iter().map(|builder| builder.children.as_slice()));
    for siblings in sibling_groups {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for sibling in siblings {
            let base = humanize_identifier(&builders[*sibling].key.module_name);
            *counts.entry(base).or_default() += 1;
        }
        let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
        for sibling in siblings {
            let base = humanize_identifier(&builders[*sibling].key.module_name);
            if counts[&base] == 1 {
                names[*sibling] = base;
                continue;
            }
            let occurrence = occurrences.entry(base.clone()).or_default();
            *occurrence += 1;
            names[*sibling] = format!("{base} {occurrence}");
        }
    }
    names
}

fn visit_semantic(index: usize, builders: &[ComponentBuilder], order: &mut Vec<usize>) {
    order.push(index);
    for child in &builders[index].children {
        visit_semantic(*child, builders, order);
    }
}

fn fallback_name(material: MaterialKey, per_color: &mut BTreeMap<[u8; 4], usize>) -> String {
    let occurrence = per_color.entry(material.rgba).or_default();
    *occurrence += 1;
    format!("{} Shape {occurrence}", rgba_hex(material.rgba))
}

fn compile_glb<'a>(structured: &'a StructuredMesh) -> CompiledGlb<'a> {
    let sources = [MeshSource {
        exact: &structured.exact,
        attributions: &structured.attributions,
    }];

    let mut builders: Vec<ComponentBuilder> = Vec::new();
    let mut by_path: BTreeMap<Vec<SemanticFrameKey>, usize> = BTreeMap::new();
    let mut anonymous: Vec<(MeshSource<'a>, MaterialKey, MeshSelection)> = Vec::new();

    for (source_index, source) in sources.iter().copied().enumerate() {
        let mut anonymous_buckets: BTreeMap<MaterialKey, Vec<u32>> = BTreeMap::new();
        for (triangle, surface_id) in source.exact.surface_ids.iter().enumerate() {
            let surface = &source.attributions[surface_id.0 as usize];
            let path = super::structured::authored_path(&surface.provenance);
            let material = MaterialKey::from_surface(surface);
            if path.is_empty() {
                anonymous_buckets
                    .entry(material)
                    .or_default()
                    .push(triangle as u32);
                continue;
            }

            let mut semantic_path = Vec::with_capacity(path.len());
            let mut parent = None;
            for frame in path {
                semantic_path.push(SemanticFrameKey::from(&frame));
                let index = if let Some(index) = by_path.get(&semantic_path) {
                    *index
                } else {
                    let index = builders.len();
                    builders.push(ComponentBuilder {
                        key: semantic_path.last().unwrap().clone(),
                        frame,
                        parent,
                        children: Vec::new(),
                        sources: BTreeMap::new(),
                        attribution: AttributionStatus::Exact,
                        contributors: Vec::new(),
                    });
                    by_path.insert(semantic_path.clone(), index);
                    if let Some(parent) = parent {
                        builders[parent].children.push(index);
                    }
                    index
                };
                parent = Some(index);
            }

            let owner = parent.expect("non-empty semantic path has an owner");
            let triangles = builders[owner].sources.entry(source_index).or_default();
            triangles
                .by_material
                .entry(material)
                .or_default()
                .push(triangle as u32);
            triangles.all.push(triangle as u32);
            if surface.status == AttributionStatus::Ambiguous {
                builders[owner].attribution = AttributionStatus::Ambiguous;
            }
            builders[owner]
                .contributors
                .extend(surface.contributors.iter().cloned());
        }

        for (material, triangles) in anonymous_buckets {
            for component in
                super::structured::components_for_triangles(&source.exact.mesh, &triangles)
            {
                anonymous.push((
                    source,
                    material,
                    super::structured::make_selection(source.exact, source.attributions, component),
                ));
            }
        }
    }

    let builder_keys: Vec<_> = builders.iter().map(|builder| builder.key.clone()).collect();
    for builder in &mut builders {
        builder
            .children
            .sort_by(|left, right| builder_keys[*left].cmp(&builder_keys[*right]));
        builder
            .contributors
            .sort_by(|left, right| super::structured::compare_provenance_paths(left, right));
        builder.contributors.dedup();
    }
    let mut semantic_roots: Vec<_> = builders
        .iter()
        .enumerate()
        .filter_map(|(index, builder)| builder.parent.is_none().then_some(index))
        .collect();
    semantic_roots.sort_by(|left, right| builders[*left].key.cmp(&builders[*right].key));
    let semantic_names = semantic_sibling_names(&builders, &semantic_roots);

    let mut old_order = Vec::with_capacity(builders.len());
    for root in &semantic_roots {
        visit_semantic(*root, &builders, &mut old_order);
    }
    let old_to_new: HashMap<_, _> = old_order
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, new))
        .collect();
    let mut components = Vec::with_capacity(builders.len() + anonymous.len());
    for old in &old_order {
        let builder = &builders[*old];
        let mut primitives = Vec::new();
        let mut edge_groups = Vec::new();
        for (source_index, triangles) in &builder.sources {
            let source = sources[*source_index];
            for (material, selected) in &triangles.by_material {
                primitives.push(GlbPrimitive {
                    source,
                    selection: super::structured::make_selection(
                        source.exact,
                        source.attributions,
                        selected.clone(),
                    ),
                    material: *material,
                });
            }
            edge_groups.push(GlbEdgeGroup {
                source,
                selection: super::structured::make_selection(
                    source.exact,
                    source.attributions,
                    triangles.all.clone(),
                ),
            });
        }
        components.push(GlbComponent {
            frame: Some(builder.frame.clone()),
            children: builder
                .children
                .iter()
                .map(|child| old_to_new[child])
                .collect(),
            name: semantic_names[*old].clone(),
            primitives,
            edge_groups,
            attribution: builder.attribution,
            contributors: builder.contributors.clone(),
        });
    }

    anonymous.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| super::structured::spatial_order(&left.2, &right.2))
    });
    let mut roots: Vec<_> = semantic_roots.iter().map(|root| old_to_new[root]).collect();
    let mut per_color = BTreeMap::new();
    for (source, material, selection) in anonymous {
        let index = components.len();
        roots.push(index);
        let name = fallback_name(material, &mut per_color);
        let mut contributors: Vec<_> = selection
            .triangles
            .iter()
            .flat_map(|triangle| {
                let surface_id = source.exact.surface_ids[*triangle as usize];
                source.attributions[surface_id.0 as usize]
                    .contributors
                    .iter()
                    .cloned()
            })
            .collect();
        contributors
            .sort_by(|left, right| super::structured::compare_provenance_paths(left, right));
        contributors.dedup();
        let attribution = selection.attribution;
        components.push(GlbComponent {
            frame: None,
            children: Vec::new(),
            name,
            primitives: vec![GlbPrimitive {
                source,
                selection: selection.clone(),
                material,
            }],
            edge_groups: vec![GlbEdgeGroup { source, selection }],
            attribution,
            contributors,
        });
    }

    CompiledGlb { components, roots }
}

fn validate_finite(structured: &StructuredMesh, scale: f64) -> Result<(), GeomError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(GeomError::Invariant(
            "source_unit_to_meters must be positive and finite".to_string(),
        ));
    }
    if structured
        .exact
        .mesh
        .verts
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(GeomError::Invariant(
            "cannot export non-finite geometry".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

fn gltf_point(point: [f64; 3], scale: f64, coordinates: CoordinateSystem) -> [f32; 3] {
    let point = point.map(|component| canonical_zero(component * scale) as f32);
    match coordinates {
        CoordinateSystem::YUp => [point[0], point[2], -point[1]],
        CoordinateSystem::ZUp => point,
    }
}

fn gltf_normal(normal: [f64; 3], coordinates: CoordinateSystem) -> [f32; 3] {
    match coordinates {
        CoordinateSystem::YUp => [normal[0] as f32, normal[2] as f32, -normal[1] as f32],
        CoordinateSystem::ZUp => normal.map(|component| component as f32),
    }
}

fn canonical_triangles(mesh: &Mesh, selection: &MeshSelection) -> Vec<(u32, [u32; 3])> {
    let mut triangles: Vec<_> = selection
        .triangles
        .iter()
        .map(|triangle_id| {
            let mut triangle = mesh.tris[*triangle_id as usize];
            let points = triangle.map(|vertex| {
                mesh.verts[vertex as usize].map(|value| canonical_zero(value).to_bits())
            });
            let first = (1..3).fold(0, |best, next| {
                if points[next] < points[best] {
                    next
                } else {
                    best
                }
            });
            triangle.rotate_left(first);
            (*triangle_id, triangle)
        })
        .collect();
    triangles.sort_by_key(|(triangle_id, triangle)| {
        (
            triangle.map(|vertex| {
                mesh.verts[vertex as usize].map(|value| canonical_zero(value).to_bits())
            }),
            *triangle_id,
        )
    });
    triangles
}

fn rgba_hex(rgba: [u8; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        rgba[0], rgba[1], rgba[2], rgba[3]
    )
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn append_aligned(buffer: &mut Vec<u8>, bytes: &[u8]) -> (usize, usize) {
    while buffer.len() % 4 != 0 {
        buffer.push(0);
    }
    let offset = buffer.len();
    buffer.extend_from_slice(bytes);
    (offset, bytes.len())
}

fn f32_bytes(values: &[[f32; 3]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 12);
    for value in values {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    bytes
}

fn u32_bytes(values: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn add_view(views: &mut Vec<Value>, buffer: &mut Vec<u8>, bytes: &[u8], target: u32) -> usize {
    let (offset, length) = append_aligned(buffer, bytes);
    let index = views.len();
    views.push(json!({
        "buffer": 0,
        "byteLength": length,
        "byteOffset": offset,
        "target": target,
    }));
    index
}

fn vec3_min_max(values: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for value in values {
        for axis in 0..3 {
            min[axis] = min[axis].min(value[axis]);
            max[axis] = max[axis].max(value[axis]);
        }
    }
    (min, max)
}

fn add_vec3_accessor(
    accessors: &mut Vec<Value>,
    view: usize,
    values: &[[f32; 3]],
    bounds: bool,
) -> usize {
    let index = accessors.len();
    let mut value = json!({
        "bufferView": view,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC3",
    });
    if bounds {
        let (min, max) = vec3_min_max(values);
        value["min"] = json!(min);
        value["max"] = json!(max);
    }
    accessors.push(value);
    index
}

fn add_index_accessor(accessors: &mut Vec<Value>, view: usize, count: usize) -> usize {
    let index = accessors.len();
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5125,
        "count": count,
        "type": "SCALAR",
    }));
    index
}

fn span_json(span: &SourceSpan, source_keys: &[String]) -> Value {
    let key = source_keys
        .get(span.source_id.0 as usize)
        .map(|value| safe_source_key(value))
        .unwrap_or_else(|| format!("source-{}", span.source_id.0));
    json!({
        "sourceId": span.source_id.0,
        "source": key,
        "start": span.start,
        "end": span.end,
    })
}

fn safe_source_key(key: &str) -> String {
    if key == "<main>" || !std::path::Path::new(key).is_absolute() {
        return key.replace('\\', "/");
    }
    std::path::Path::new(key)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.scad")
        .to_string()
}

fn provenance_json(frames: &[ProvenanceFrame], source_keys: &[String]) -> Value {
    let mut frames = frames.to_vec();
    frames.sort_by_key(|frame| {
        (
            frame.call_site.source_id,
            frame.call_site.start,
            frame.call_site.end,
            frame.module_name.clone(),
        )
    });
    frames.dedup();
    Value::Array(
        frames
            .iter()
            .map(|frame| {
                json!({
                    "callSite": span_json(&frame.call_site, source_keys),
                    "definitionSite": frame.definition_site.as_ref().map(|span| span_json(span, source_keys)),
                    "moduleName": frame.module_name,
                })
            })
            .collect(),
    )
}

fn triangle_normal(mesh: &Mesh, triangle: [u32; 3]) -> [f64; 3] {
    let a = mesh.verts[triangle[0] as usize];
    let b = mesh.verts[triangle[1] as usize];
    let c = mesh.verts[triangle[2] as usize];
    normalize(cross(sub(b, a), sub(c, a)))
}

#[cfg(test)]
thread_local! {
    static EDGE_DERIVATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_edge_derivation_count() {
    EDGE_DERIVATIONS.set(0);
}

#[cfg(test)]
pub(crate) fn edge_derivation_count() -> usize {
    EDGE_DERIVATIONS.get()
}

/// Two adjacent source patches are one surface when the sharpest dihedral
/// anywhere along their shared boundary stays under this angle. Deciding per
/// pair rather than per edge keeps a boundary that is sharp somewhere drawn
/// along its whole length, including where it flattens toward tangency.
///
/// Matches the threshold Tau's cross-kernel edge middleware applies to meshes
/// that arrive without authored lines.
const PATCH_MERGE_DEGREES: f64 = 30.0;

/// Geometry with no source topology — `import`, `surface`, and the ruled bands
/// of `hull` and `minkowski` — has no patches to pair up, so each edge is
/// judged on its own dihedral. The angle is lower than [`PATCH_MERGE_DEGREES`]
/// because such meshes are routinely coarse enough that a 30° cut would erase
/// their real creases.
const RAW_CREASE_DEGREES: f64 = 20.0;

fn dihedral_degrees(mesh: &Mesh, left: usize, right: usize) -> f64 {
    let left_normal = triangle_normal(mesh, mesh.tris[left]);
    let right_normal = triangle_normal(mesh, mesh.tris[right]);
    let cosine = left_normal
        .iter()
        .zip(right_normal)
        .map(|(left, right)| left * right)
        .sum::<f64>()
        .clamp(-1.0, 1.0);
    cosine.acos().to_degrees()
}

/// Groups patches whose shared boundary is nowhere a crease, returning the
/// representative each patch resolves to.
fn merged_patches(
    mesh: &Mesh,
    source_face_ids: &[u64],
    adjacency: &BTreeMap<(u32, u32), Vec<u32>>,
) -> HashMap<u64, u64> {
    let mut sharpest: BTreeMap<(u64, u64), f64> = BTreeMap::new();
    for ((left_vertex, right_vertex), local) in adjacency {
        if local.len() != 2
            || mesh.verts[*left_vertex as usize] == mesh.verts[*right_vertex as usize]
        {
            continue;
        }
        let (left, right) = (local[0] as usize, local[1] as usize);
        let (left_patch, right_patch) = (source_face_ids[left], source_face_ids[right]);
        if left_patch == right_patch
            || left_patch == super::structured::UNCLASSIFIED_PATCH_ID
            || right_patch == super::structured::UNCLASSIFIED_PATCH_ID
        {
            continue;
        }
        let angle = dihedral_degrees(mesh, left, right);
        let entry = sharpest
            .entry((left_patch.min(right_patch), left_patch.max(right_patch)))
            .or_insert(0.0);
        if angle > *entry {
            *entry = angle;
        }
    }
    if sharpest.is_empty() {
        return HashMap::new();
    }

    let mut slots: HashMap<u64, usize> = HashMap::new();
    let mut patches: Vec<u64> = Vec::new();
    let mut slot_of = |patch: u64, patches: &mut Vec<u64>| {
        *slots.entry(patch).or_insert_with(|| {
            patches.push(patch);
            patches.len() - 1
        })
    };
    let pairs: Vec<_> = sharpest
        .iter()
        .filter(|(_, angle)| **angle < PATCH_MERGE_DEGREES)
        .map(|((left, right), _)| (slot_of(*left, &mut patches), slot_of(*right, &mut patches)))
        .collect();

    let mut union = super::structured::UnionFind::new(patches.len());
    for (left, right) in pairs {
        union.union(left, right);
    }
    patches
        .iter()
        .enumerate()
        .map(|(slot, patch)| (*patch, patches[union.find(slot)]))
        .collect()
}

fn feature_edges(source: MeshSource<'_>, selection: &MeshSelection) -> Vec<[u32; 2]> {
    #[cfg(test)]
    EDGE_DERIVATIONS.set(EDGE_DERIVATIONS.get() + 1);

    let mesh = &source.exact.mesh;
    let selected: BTreeSet<_> = selection.triangles.iter().copied().collect();
    let mut adjacency: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for triangle_id in &selection.triangles {
        let triangle = mesh.tris[*triangle_id as usize];
        for edge in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let key = if edge[0] < edge[1] {
                (edge[0], edge[1])
            } else {
                (edge[1], edge[0])
            };
            adjacency.entry(key).or_default().push(*triangle_id);
        }
    }
    let roots = merged_patches(mesh, &source.exact.source_face_ids, &adjacency);
    let resolve = |patch: u64| roots.get(&patch).copied().unwrap_or(patch);

    let mut result = Vec::new();
    for ((a, b), local) in adjacency {
        if mesh.verts[a as usize] == mesh.verts[b as usize] {
            continue;
        }
        let include = if local.len() != 2 {
            true
        } else {
            let left = local[0] as usize;
            let right = local[1] as usize;
            let left_patch = source.exact.source_face_ids[left];
            let right_patch = source.exact.source_face_ids[right];
            if left_patch != super::structured::UNCLASSIFIED_PATCH_ID
                && right_patch != super::structured::UNCLASSIFIED_PATCH_ID
            {
                resolve(left_patch) != resolve(right_patch)
            } else {
                dihedral_degrees(mesh, left, right) >= RAW_CREASE_DEGREES
            }
        };
        if include && local.iter().all(|triangle| selected.contains(triangle)) {
            result.push([a, b]);
        }
    }
    result.sort_by_key(|edge| {
        let mut points = edge
            .map(|vertex| mesh.verts[vertex as usize].map(|value| canonical_zero(value).to_bits()));
        points.sort_unstable();
        points
    });
    result.dedup_by_key(|edge| {
        let mut points = edge
            .map(|vertex| mesh.verts[vertex as usize].map(|value| canonical_zero(value).to_bits()));
        points.sort_unstable();
        points
    });
    result
}

pub(crate) fn serialize_glb(
    structured: &StructuredMesh,
    options: &Export3DOptions,
) -> Result<Vec<u8>, GeomError> {
    validate_finite(structured, options.source_unit_to_meters)?;

    let compiled = compile_glb(structured);
    let mut colors: Vec<_> = compiled
        .components
        .iter()
        .flat_map(|component| component.primitives.iter())
        .map(|primitive| material_rgba_from_key(primitive.material))
        .collect();
    colors.sort_unstable();
    colors.dedup();
    let material_by_color: BTreeMap<_, _> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| (*color, index))
        .collect();
    let mut materials: Vec<Value> = colors
        .iter()
        .map(|rgba| {
            let mut material = json!({
                "name": format!("{} Material", rgba_hex(*rgba)),
                "pbrMetallicRoughness": {
                    "baseColorFactor": [
                        srgb_to_linear(rgba[0]),
                        srgb_to_linear(rgba[1]),
                        srgb_to_linear(rgba[2]),
                        f32::from(rgba[3]) / 255.0,
                    ],
                    "metallicFactor": 0.1,
                    "roughnessFactor": 0.6,
                }
            });
            material["doubleSided"] = json!(true);
            if rgba[3] < 255 {
                material["alphaMode"] = json!("BLEND");
            }
            material
        })
        .collect();
    let line_material = options.include_edges.then(|| {
        let index = materials.len();
        materials.push(json!({
            "name": "Feature Edges",
            "extensions": { "KHR_materials_unlit": {} },
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.0, 0.0, 0.0, 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
            "alphaMode": "OPAQUE",
            "doubleSided": true,
        }));
        index
    });

    let mut buffer = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    for component in &compiled.components {
        let mut primitives = Vec::new();
        for primitive in &component.primitives {
            let triangles = canonical_triangles(&primitive.source.exact.mesh, &primitive.selection);
            let mut positions = Vec::with_capacity(triangles.len() * 3);
            let mut normals = Vec::with_capacity(triangles.len() * 3);
            let mut indices = Vec::with_capacity(triangles.len() * 3);
            for (_, triangle) in &triangles {
                let normal = triangle_normal(&primitive.source.exact.mesh, *triangle);
                // A zero-area triangle has no normal, and `normalize` says so with
                // a zero vector — which glTF rejects, because NORMAL must be unit
                // length. The direction is arbitrary for a face with no area, so
                // pick one rather than write an invalid file. Only the attribute
                // substitutes: `triangle_normal` stays honest for the dihedral
                // comparison, where a degenerate face reading as a crease is the
                // conservative answer.
                let normal = if normal == [0.0, 0.0, 0.0] {
                    [0.0, 0.0, 1.0]
                } else {
                    normal
                };
                let normal = gltf_normal(normal, options.coordinate_system);
                for vertex in triangle {
                    positions.push(gltf_point(
                        primitive.source.exact.mesh.verts[*vertex as usize],
                        options.source_unit_to_meters,
                        options.coordinate_system,
                    ));
                    normals.push(normal);
                    indices.push(indices.len() as u32);
                }
            }
            let position_view = add_view(&mut views, &mut buffer, &f32_bytes(&positions), 34962);
            let normal_view = add_view(&mut views, &mut buffer, &f32_bytes(&normals), 34962);
            let index_view = add_view(&mut views, &mut buffer, &u32_bytes(&indices), 34963);
            let position_accessor =
                add_vec3_accessor(&mut accessors, position_view, &positions, true);
            let normal_accessor = add_vec3_accessor(&mut accessors, normal_view, &normals, false);
            let index_accessor = add_index_accessor(&mut accessors, index_view, indices.len());
            let display_mode = primitive.material.display_mode();
            let mut primitive = json!({
                "attributes": { "NORMAL": normal_accessor, "POSITION": position_accessor },
                "indices": index_accessor,
                "material": material_by_color[&material_rgba_from_key(primitive.material)],
                "mode": 4,
            });
            if structured.include_display_modes {
                primitive["extras"] = json!({
                    "openrscad": { "displayMode": display_mode }
                });
            }
            primitives.push(primitive);
        }

        if let Some(line_material) = line_material {
            for edge_group in &component.edge_groups {
                #[cfg(feature = "benchmark-profile")]
                let edge_started = Instant::now();
                let edges = feature_edges(edge_group.source, &edge_group.selection);
                #[cfg(feature = "benchmark-profile")]
                super::structured::record_edge_derivation(
                    edge_started.elapsed().as_secs_f64() * 1_000.0,
                    edges.len(),
                );
                if edges.is_empty() {
                    continue;
                }
                let mut line_positions = Vec::with_capacity(edges.len() * 2);
                let mut line_indices = Vec::with_capacity(edges.len() * 2);
                for edge in edges {
                    for vertex in edge {
                        line_positions.push(gltf_point(
                            edge_group.source.exact.mesh.verts[vertex as usize],
                            options.source_unit_to_meters,
                            options.coordinate_system,
                        ));
                        line_indices.push(line_indices.len() as u32);
                    }
                }
                let position_view =
                    add_view(&mut views, &mut buffer, &f32_bytes(&line_positions), 34962);
                let index_view =
                    add_view(&mut views, &mut buffer, &u32_bytes(&line_indices), 34963);
                let position_accessor =
                    add_vec3_accessor(&mut accessors, position_view, &line_positions, true);
                let index_accessor =
                    add_index_accessor(&mut accessors, index_view, line_indices.len());
                // `mode: 1` and the shared Feature Edges material already
                // identify a generated edge primitive; no consumer read the
                // extras tag that used to sit here.
                primitives.push(json!({
                    "attributes": { "POSITION": position_accessor },
                    "indices": index_accessor,
                    "material": line_material,
                    "mode": 1,
                }));
            }
        }

        let mesh_index = if primitives.is_empty() {
            None
        } else {
            let mesh_index = meshes.len();
            meshes.push(json!({ "name": component.name, "primitives": primitives }));
            Some(mesh_index)
        };
        let attribution = match component.attribution {
            AttributionStatus::Exact => "exact",
            AttributionStatus::Ambiguous => "ambiguous",
        };
        let mut openrscad = if let Some(frame) = &component.frame {
            json!({
                "attribution": attribution,
                "callSite": span_json(&frame.call_site, &options.source_keys),
                "definitionSite": frame.definition_site.as_ref().map(|span| span_json(span, &options.source_keys)),
                "moduleName": frame.module_name,
            })
        } else {
            let mut provenance = Vec::new();
            for primitive in &component.primitives {
                for frame in &primitive.selection.provenance {
                    if !provenance.contains(frame) {
                        provenance.push(frame.clone());
                    }
                }
            }
            json!({
                "attribution": attribution,
                "fallback": true,
                "provenance": provenance_json(&provenance, &options.source_keys),
            })
        };
        if !component.contributors.is_empty() {
            openrscad["contributors"] = Value::Array(
                component
                    .contributors
                    .iter()
                    .map(|path| provenance_json(path, &options.source_keys))
                    .collect(),
            );
        }
        let mut node = json!({
            "extras": { "openrscad": openrscad },
            "name": component.name,
        });
        if let Some(mesh_index) = mesh_index {
            node["mesh"] = json!(mesh_index);
        }
        if !component.children.is_empty() {
            node["children"] = json!(component.children);
        }
        nodes.push(node);
    }
    while buffer.len() % 4 != 0 {
        buffer.push(0);
    }
    let mut scene = json!({});
    if !compiled.roots.is_empty() {
        scene["nodes"] = json!(compiled.roots);
    }
    let mut document = json!({
        "asset": {
            "generator": "OpenRSCAD",
            "version": "2.0",
            "extras": {
                "openrscad": {
                    "schemaVersion": 1,
                    "status": "experimental",
                    "sources": options.source_keys.iter().enumerate().map(|(source_id, source)| {
                        json!({ "sourceId": source_id, "source": safe_source_key(source) })
                    }).collect::<Vec<_>>(),
                }
            }
        },
        "scene": 0,
        "scenes": [scene],
    });
    // glTF gives every one of these arrays `minItems: 1`, so a model that
    // renders to nothing — an empty difference, a collapsed offset — has to omit
    // them rather than write them empty. Same for the BIN chunk below: a
    // zero-length buffer is invalid, not merely wasteful.
    for (key, value) in [
        ("accessors", json!(accessors)),
        ("bufferViews", json!(views)),
        ("materials", json!(materials)),
        ("meshes", json!(meshes)),
        ("nodes", json!(nodes)),
    ] {
        if !value.as_array().is_some_and(|entries| entries.is_empty()) {
            document[key] = value;
        }
    }
    if !buffer.is_empty() {
        document["buffers"] = json!([{ "byteLength": buffer.len() }]);
    }
    if options.include_edges {
        document["extensionsUsed"] = json!(["KHR_materials_unlit"]);
    }
    let mut json_bytes = serde_json::to_vec(&document)
        .map_err(|error| GeomError::Invariant(format!("GLB JSON serialization failed: {error}")))?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let binary_chunk = if buffer.is_empty() {
        0
    } else {
        8 + buffer.len()
    };
    let total = 12 + 8 + json_bytes.len() + binary_chunk;
    let total =
        u32::try_from(total).map_err(|_| GeomError::Invariant("GLB exceeds 4 GiB".to_string()))?;
    let mut glb = Vec::with_capacity(total as usize);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&total.to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    if !buffer.is_empty() {
        glb.extend_from_slice(&(buffer.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&buffer);
    }
    Ok(glb)
}

fn xml_number(value: f64) -> Result<String, GeomError> {
    if !value.is_finite() {
        return Err(GeomError::Invariant(
            "cannot export non-finite geometry".to_string(),
        ));
    }
    Ok(canonical_zero(value).to_string())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

type ColoredTriangle = ([u32; 3], [u8; 4]);

fn compact_component(
    structured: &StructuredMesh,
    selection: &MeshSelection,
) -> (Vec<[f64; 3]>, Vec<ColoredTriangle>) {
    let triangles = canonical_triangles(&structured.exact.mesh, selection);
    let mut remap = HashMap::new();
    let mut vertices = Vec::new();
    let mut compact = Vec::with_capacity(triangles.len());
    for (triangle_id, triangle) in triangles {
        let triangle = triangle.map(|vertex| {
            *remap.entry(vertex).or_insert_with(|| {
                vertices.push(structured.exact.mesh.verts[vertex as usize]);
                vertices.len() as u32 - 1
            })
        });
        let surface =
            &structured.attributions[structured.exact.surface_ids[triangle_id as usize].0 as usize];
        compact.push((triangle, rgba_bytes(surface.rgba)));
    }
    (vertices, compact)
}

fn validate_closed_component(mesh: &Mesh, selection: &MeshSelection) -> Result<(), GeomError> {
    let mut edges: BTreeMap<(u32, u32), (usize, i32)> = BTreeMap::new();
    for triangle_id in &selection.triangles {
        let triangle = mesh.tris[*triangle_id as usize];
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            return Err(GeomError::NonManifold(
                "3MF object contains a degenerate triangle".to_string(),
            ));
        }
        for [from, to] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let key = if from < to { (from, to) } else { (to, from) };
            let edge = edges.entry(key).or_default();
            edge.0 += 1;
            edge.1 += if from < to { 1 } else { -1 };
        }
    }
    if let Some((edge, incidence)) = edges
        .into_iter()
        .find(|(_, (count, orientation))| *count != 2 || *orientation != 0)
    {
        return Err(GeomError::NonManifold(format!(
            "3MF object edge {}-{} has {} incident triangles and orientation balance {}",
            edge.0, edge.1, incidence.0, incidence.1
        )));
    }
    Ok(())
}

fn solid_semantic_paths(
    structured: &StructuredMesh,
    selection: &MeshSelection,
) -> Vec<Vec<ProvenanceFrame>> {
    let mut paths: Vec<_> = selection
        .triangles
        .iter()
        .flat_map(|triangle| {
            let surface_id = structured.exact.surface_ids[*triangle as usize];
            let surface = &structured.attributions[surface_id.0 as usize];
            if surface.contributors.is_empty() {
                vec![super::structured::authored_path(&surface.provenance)]
            } else {
                surface.contributors.clone()
            }
        })
        .collect();
    paths.sort_by(|left, right| super::structured::compare_provenance_paths(left, right));
    paths.dedup();
    paths
}

fn solid_base_name(
    structured: &StructuredMesh,
    selection: &MeshSelection,
    paths: &[Vec<ProvenanceFrame>],
) -> String {
    if paths.len() == 1 {
        if let Some(module_name) = paths[0]
            .last()
            .and_then(|frame| frame.module_name.as_deref())
        {
            return humanize_identifier(module_name);
        }
        let triangle = selection.triangles[0] as usize;
        let surface = &structured.attributions[structured.exact.surface_ids[triangle].0 as usize];
        return format!("{} Shape", rgba_hex(rgba_bytes(surface.rgba)));
    }
    super::structured::longest_common_prefix(paths)
        .last()
        .and_then(|frame| frame.module_name.as_deref())
        .map(|module_name| format!("{} Solid", humanize_identifier(module_name)))
        .unwrap_or_else(|| "Shape".to_string())
}

fn solid_names(structured: &StructuredMesh, paths: &[Vec<Vec<ProvenanceFrame>>]) -> Vec<String> {
    let bases: Vec<_> = paths
        .iter()
        .zip(&structured.solid_components)
        .map(|(paths, selection)| solid_base_name(structured, selection, paths))
        .collect();
    let mut counts = BTreeMap::new();
    for base in &bases {
        *counts.entry(base.clone()).or_insert(0usize) += 1;
    }
    let mut occurrences = BTreeMap::new();
    bases
        .into_iter()
        .map(|base| {
            if counts[&base] == 1 {
                return base;
            }
            let occurrence = occurrences.entry(base.clone()).or_insert(0usize);
            *occurrence += 1;
            format!("{base} {occurrence}")
        })
        .collect()
}

fn ordered_provenance_json(frames: &[ProvenanceFrame], source_keys: &[String]) -> Value {
    Value::Array(
        frames
            .iter()
            .map(|frame| {
                json!({
                    "callSite": span_json(&frame.call_site, source_keys),
                    "definitionSite": frame.definition_site.as_ref().map(|span| span_json(span, source_keys)),
                    "moduleName": frame.module_name,
                })
            })
            .collect(),
    )
}

pub(crate) fn serialize_3mf(
    structured: &StructuredMesh,
    options: &Export3DOptions,
) -> Result<Vec<u8>, GeomError> {
    validate_finite(structured, 1.0)?;
    let mut colors: Vec<_> = structured
        .exact
        .surface_ids
        .iter()
        .map(|id| rgba_bytes(structured.attributions[id.0 as usize].rgba))
        .collect();
    colors.sort_unstable();
    colors.dedup();
    let material_by_color: BTreeMap<_, _> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| (*color, index))
        .collect();
    let semantic_paths: Vec<_> = structured
        .solid_components
        .iter()
        .map(|component| solid_semantic_paths(structured, component))
        .collect();
    let names = solid_names(structured, &semantic_paths);
    let source_table = serde_json::to_string(
        &options
            .source_keys
            .iter()
            .enumerate()
            .map(|(source_id, source)| {
                json!({ "sourceId": source_id, "source": safe_source_key(source) })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|error| GeomError::Invariant(format!("3MF source serialization failed: {error}")))?;
    let mut model = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:openrscad=\"https://openrscad.com/3mf/metadata/1\">\n\
         <metadata name=\"openrscad:schemaVersion\" preserve=\"true\">1</metadata>\n\
         <metadata name=\"openrscad:status\" preserve=\"true\">experimental</metadata>\n",
    );
    model.push_str(&format!(
        "<metadata name=\"openrscad:sourceTable\" preserve=\"true\">{}</metadata><resources>\n",
        xml_escape(&source_table)
    ));
    if !colors.is_empty() {
        model.push_str("<basematerials id=\"1\">\n");
        for color in &colors {
            model.push_str(&format!(
                "<base name=\"{}\" displaycolor=\"#{:02X}{:02X}{:02X}{:02X}\"/>\n",
                xml_escape(&rgba_hex(*color)),
                color[0],
                color[1],
                color[2],
                color[3]
            ));
        }
        model.push_str("</basematerials>\n");
    }
    for (component_index, component) in structured.solid_components.iter().enumerate() {
        validate_closed_component(&structured.exact.mesh, component)?;
        let (vertices, triangles) = compact_component(structured, component);
        if triangles.is_empty() {
            continue;
        }
        let object_id = component_index + 2;
        let default_material = material_by_color[&triangles[0].1];
        let attribution = match component.attribution {
            AttributionStatus::Exact => "exact",
            AttributionStatus::Ambiguous => "ambiguous",
        };
        let owners = Value::Array(
            semantic_paths[component_index]
                .iter()
                .map(|path| ordered_provenance_json(path, &options.source_keys))
                .collect(),
        );
        let owners = serde_json::to_string(&owners).map_err(|error| {
            GeomError::Invariant(format!("3MF provenance serialization failed: {error}"))
        })?;
        model.push_str(&format!(
            "<object id=\"{object_id}\" type=\"model\" name=\"{}\" pid=\"1\" pindex=\"{default_material}\"><metadatagroup><metadata name=\"openrscad:semanticOwners\" preserve=\"true\">{}</metadata><metadata name=\"openrscad:attribution\" preserve=\"true\">{attribution}</metadata></metadatagroup><mesh><vertices>\n",
            xml_escape(&names[component_index]),
            xml_escape(&owners),
        ));
        for vertex in vertices {
            model.push_str(&format!(
                "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>\n",
                xml_number(vertex[0])?,
                xml_number(vertex[1])?,
                xml_number(vertex[2])?
            ));
        }
        model.push_str("</vertices><triangles>\n");
        for (triangle, color) in triangles {
            let material = material_by_color[&color];
            model.push_str(&format!(
                "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{material}\" p2=\"{material}\" p3=\"{material}\"/>\n",
                triangle[0], triangle[1], triangle[2]
            ));
        }
        model.push_str("</triangles></mesh></object>\n");
    }
    model.push_str("</resources><build>\n");
    for component_index in 0..structured.solid_components.len() {
        model.push_str(&format!("<item objectid=\"{}\"/>\n", component_index + 2));
    }
    model.push_str("</build></model>\n");
    Ok(package_3mf(&model))
}

#[cfg(test)]
pub(crate) fn parse_glb_json(bytes: &[u8]) -> Value {
    assert!(bytes.len() >= 20 && &bytes[..4] == b"glTF");
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + length]).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_structured_rust_cached, GeomCache, RustManifoldKernel};
    use openrscad_ir::{FragmentSpec, Node, ProvenanceFrame, SourceId, SourceSpan};

    fn glb_json(bytes: &[u8]) -> serde_json::Value {
        let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&bytes[20..20 + length]).unwrap()
    }

    fn line_segment_count(document: &Value) -> u64 {
        document["meshes"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|mesh| mesh["primitives"].as_array().into_iter().flatten())
            .filter(|primitive| primitive["mode"] == 1)
            .map(|primitive| {
                let accessor = primitive["indices"].as_u64().unwrap() as usize;
                document["accessors"][accessor]["count"].as_u64().unwrap() / 2
            })
            .sum()
    }

    fn two_cubes() -> StructuredMesh {
        render_structured_rust_cached(
            &Node::Union(vec![
                Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                },
                Node::Translate {
                    v: [0.0, 0.0, 2.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
            ]),
            &RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap()
    }

    fn authored(name: &str, call_start: u32, child: Node) -> Node {
        Node::Provenance {
            frame: ProvenanceFrame {
                call_site: SourceSpan {
                    source_id: SourceId(0),
                    start: call_start,
                    end: call_start + name.len() as u32,
                },
                definition_site: Some(SourceSpan {
                    source_id: SourceId(0),
                    start: call_start + 1_000,
                    end: call_start + 1_100,
                }),
                module_name: Some(name.to_string()),
            },
            child: Box::new(child),
        }
    }

    fn rendered(node: &Node) -> StructuredMesh {
        render_structured_rust_cached(node, &RustManifoldKernel::new(), &mut GeomCache::new())
            .unwrap()
    }

    fn surface_triangle_count(document: &Value) -> u64 {
        document["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|mesh| mesh["primitives"].as_array().unwrap())
            .filter(|primitive| primitive["mode"] == 4)
            .map(|primitive| {
                let accessor = primitive["indices"].as_u64().unwrap() as usize;
                document["accessors"][accessor]["count"].as_u64().unwrap() / 3
            })
            .sum()
    }

    fn node_surface_triangle_count(document: &Value, node_index: usize) -> u64 {
        let Some(mesh_index) = document["nodes"][node_index]["mesh"].as_u64() else {
            return 0;
        };
        document["meshes"][mesh_index as usize]["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|primitive| primitive["mode"] == 4)
            .map(|primitive| {
                let accessor = primitive["indices"].as_u64().unwrap() as usize;
                document["accessors"][accessor]["count"].as_u64().unwrap() / 3
            })
            .sum()
    }

    fn threemf_document(bytes: &[u8]) -> String {
        crate::mesh::read_3mf_model(bytes).unwrap()
    }

    fn threemf_object_names(model: &roxmltree::Document<'_>) -> Vec<String> {
        model
            .descendants()
            .filter(|node| node.has_tag_name("object"))
            .map(|node| node.attribute("name").unwrap().to_string())
            .collect()
    }

    #[test]
    fn threemf_rejects_open_physical_objects() {
        let scene = AttributedMesh {
            mesh: Mesh {
                verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                tris: vec![[0, 1, 2]],
            },
            surface_ids: vec![super::super::SurfaceAttributionId(0)],
            source_face_ids: vec![0],
        };
        let selection = MeshSelection {
            triangles: vec![0],
            bounds: super::super::structured::Bounds3 {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 0.0],
            },
            provenance: Vec::new(),
            attribution: AttributionStatus::Exact,
            geometry_hash: 0,
        };
        let structured = StructuredMesh {
            aggregate: scene.mesh.clone(),
            exact: scene,
            include_display_modes: false,
            solid_components: vec![selection],
            attributions: vec![SurfaceAttribution {
                rgba: [1.0, 0.0, 0.0, 1.0],
                mode: DisplayMode::Solid,
                provenance: Vec::new(),
                contributors: Vec::new(),
                contribution: super::super::ContributionKind::Boundary,
                status: AttributionStatus::Exact,
            }],
        };

        let error = serialize_3mf(&structured, &Export3DOptions::default()).unwrap_err();
        assert!(matches!(error, GeomError::NonManifold(_)));
    }

    #[test]
    fn touching_authored_modules_emit_exact_nested_semantic_nodes() {
        let node = authored(
            "assembly",
            10,
            Node::Union(vec![
                authored(
                    "lower_frame",
                    20,
                    Node::Cube {
                        size: [10.0, 10.0, 2.0],
                        center: false,
                    },
                ),
                authored(
                    "roof_frame",
                    30,
                    Node::Translate {
                        v: [0.0, 0.0, 2.0],
                        child: Box::new(Node::Cube {
                            size: [10.0, 10.0, 2.0],
                            center: false,
                        }),
                    },
                ),
            ]),
        );
        let structured = rendered(&node);
        assert_eq!(structured.exact.mesh.tris.len(), 20);

        let bytes = serialize_glb(&structured, &Export3DOptions::default()).unwrap();
        let document = glb_json(&bytes);
        assert_eq!(surface_triangle_count(&document), 20);
        assert_eq!(document["scenes"][0]["nodes"], json!([0]));
        assert_eq!(document["nodes"][0]["name"], "Assembly");
        assert_eq!(document["nodes"][0]["children"], json!([1, 2]));
        assert_eq!(document["nodes"][1]["name"], "Lower Frame");
        assert_eq!(document["nodes"][2]["name"], "Roof Frame");
        assert_eq!(
            document["nodes"][2]["extras"]["openrscad"]["moduleName"],
            "roof_frame"
        );
        assert_eq!(
            document["nodes"][2]["extras"]["openrscad"]["callSite"],
            json!({ "sourceId": 0, "source": "<main>", "start": 30, "end": 40 })
        );
    }

    #[test]
    fn overlapping_structural_siblings_keep_complete_authored_geometry() {
        let node = Node::Group(vec![
            authored(
                "left_part",
                10,
                Node::Cube {
                    size: [2.0, 2.0, 2.0],
                    center: false,
                },
            ),
            authored(
                "right_part",
                20,
                Node::Translate {
                    v: [1.0, 0.0, 0.0],
                    child: Box::new(Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    }),
                },
            ),
        ]);
        let structured = rendered(&node);
        let document = glb_json(&serialize_glb(&structured, &Export3DOptions::default()).unwrap());
        let names: Vec<_> = document["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect();

        assert_eq!(names, ["Left Part", "Right Part"]);
        assert_eq!(node_surface_triangle_count(&document, 0), 12);
        assert_eq!(node_surface_triangle_count(&document, 1), 12);
        assert!((structured.aggregate.volume() - 12.0).abs() < 1e-6);

        let model_xml =
            threemf_document(&serialize_3mf(&structured, &Export3DOptions::default()).unwrap());
        let model = roxmltree::Document::parse(&model_xml).unwrap();
        assert_eq!(threemf_object_names(&model), ["Left Part", "Right Part"]);
    }

    #[test]
    fn smooth_sphere_has_no_tessellation_feature_lines() {
        let structured = rendered(&Node::Sphere {
            r: 10.0,
            frags: FragmentSpec {
                fn_: 32.0,
                ..FragmentSpec::default()
            },
        });
        let document = glb_json(
            &serialize_glb(
                &structured,
                &Export3DOptions {
                    include_edges: true,
                    ..Export3DOptions::default()
                },
            )
            .unwrap(),
        );
        let primitives = document["meshes"][0]["primitives"].as_array().unwrap();

        assert_eq!(primitives.len(), 1);
        assert_eq!(primitives[0]["mode"], 4);
    }

    #[test]
    fn export_omits_preview_policy_while_render_glb_keeps_factual_display_modes() {
        let node = Node::Group(vec![
            Node::Highlight(Box::new(Node::Cube {
                size: [1.0, 1.0, 1.0],
                center: false,
            })),
            Node::Background(Box::new(Node::Translate {
                v: [0.0, 0.0, 2.0],
                child: Box::new(Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                }),
            })),
        ]);
        let export =
            glb_json(&serialize_glb(&rendered(&node), &Export3DOptions::default()).unwrap());
        let (render, _) = crate::render_structured_cached_diag(
            &node,
            &RustManifoldKernel::new(),
            &mut GeomCache::new(),
            true,
        )
        .unwrap();
        let render = glb_json(&serialize_glb(&render, &Export3DOptions::default()).unwrap());

        assert_eq!(surface_triangle_count(&export), 12);
        assert!(export["meshes"][0]["primitives"][0]["extras"].is_null());
        let modes: BTreeSet<_> = render["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|mesh| mesh["primitives"].as_array().unwrap())
            .map(|primitive| {
                primitive["extras"]["openrscad"]["displayMode"]
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(modes, BTreeSet::from(["background", "highlight"]));
    }

    #[test]
    fn glb_vendor_contract_is_declared_experimental_version_one() {
        let document = glb_json(&serialize_glb(&two_cubes(), &Export3DOptions::default()).unwrap());

        assert_eq!(document["asset"]["extras"]["openrscad"]["schemaVersion"], 1);
        assert_eq!(
            document["asset"]["extras"]["openrscad"]["status"],
            "experimental"
        );
        assert_eq!(
            document["materials"][0]["pbrMetallicRoughness"]["metallicFactor"],
            0.1
        );
        assert_eq!(
            document["materials"][0]["pbrMetallicRoughness"]["roughnessFactor"],
            0.6
        );
        assert_eq!(document["materials"][0]["doubleSided"], true);
        assert_eq!(document["materials"][0]["name"], "#F5A523FF Material");
    }

    /// `linear_extrude(2) hull() projection(cut=true) sphere(8, $fn=48)` — the
    /// corpus's `d2_proj_hull` — triangulates its caps with zero-area slivers,
    /// and a sliver has no normal to compute. glTF requires NORMAL to be unit
    /// length, so writing the honest zero vector produced a file every validator
    /// rejects.
    #[test]
    fn degenerate_faces_still_export_unit_normals() {
        let frags = FragmentSpec {
            fn_: 48.0,
            ..FragmentSpec::default()
        };
        let structured = rendered(&Node::LinearExtrude {
            height: 2.0,
            center: false,
            twist: 0.0,
            scale: [1.0, 1.0],
            slices: 1,
            child: Box::new(Node::Hull(vec![Node::Projection {
                cut: true,
                child: Box::new(Node::Sphere { r: 8.0, frags }),
            }])),
        });
        let bytes = export_3d(
            &structured,
            ExportFormat3D::Glb,
            &Export3DOptions::default(),
        )
        .unwrap()
        .bytes;
        let document = glb_json(&bytes);

        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let binary = &bytes[20 + json_length + 8..];
        let mut checked = 0;
        for mesh in document["meshes"].as_array().unwrap() {
            for primitive in mesh["primitives"].as_array().unwrap() {
                let Some(accessor_index) = primitive["attributes"]["NORMAL"].as_u64() else {
                    continue;
                };
                let accessor = &document["accessors"][accessor_index as usize];
                let view =
                    &document["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
                let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
                for index in 0..accessor["count"].as_u64().unwrap() as usize {
                    let at = start + index * 12;
                    let component = |offset: usize| {
                        f32::from_le_bytes(binary[at + offset..at + offset + 4].try_into().unwrap())
                            as f64
                    };
                    let length =
                        (component(0).powi(2) + component(4).powi(2) + component(8).powi(2)).sqrt();
                    assert!(
                        (length - 1.0).abs() < 1e-5,
                        "normal {index} has length {length}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }

    /// Deliberately empty models are in the corpus — an empty difference, a
    /// collapsed offset — and the writer used to hand them a GLB with empty
    /// arrays and a zero-length BIN chunk, which the Khronos validator rejects
    /// outright. Nothing is a legal thing to export; an invalid file is not.
    #[test]
    fn a_model_that_renders_to_nothing_exports_a_valid_empty_document() {
        let structured = rendered(&Node::Difference(vec![
            Node::Cube {
                size: [1.0, 1.0, 1.0],
                center: true,
            },
            Node::Cube {
                size: [4.0, 4.0, 4.0],
                center: true,
            },
        ]));
        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };
        let bytes = export_3d(&structured, ExportFormat3D::Glb, &options)
            .unwrap()
            .bytes;
        let document = glb_json(&bytes);

        for key in ["accessors", "bufferViews", "buffers", "meshes", "nodes"] {
            assert!(
                document.get(key).is_none(),
                "{key} must be omitted, not empty"
            );
        }
        assert!(document["scenes"][0].get("nodes").is_none());
        assert_eq!(document["scene"], 0);
        // Header plus one JSON chunk, and no BIN chunk at all.
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 12 + 8 + json_length);
    }

    #[test]
    fn smooth_extrusions_do_not_emit_tessellation_edges() {
        let circle = || Node::Circle {
            r: 10.0,
            frags: FragmentSpec {
                fn_: 16.0,
                ..FragmentSpec::default()
            },
        };
        let linear = rendered(&Node::LinearExtrude {
            height: 10.0,
            center: false,
            twist: 0.0,
            scale: [1.0, 1.0],
            slices: 1,
            child: Box::new(circle()),
        });
        let revolved = rendered(&Node::RotateExtrude {
            angle: 360.0,
            frags: FragmentSpec {
                fn_: 16.0,
                ..FragmentSpec::default()
            },
            child: Box::new(Node::Translate {
                v: [20.0, 0.0, 0.0],
                child: Box::new(circle()),
            }),
        });
        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };

        assert_eq!(
            line_segment_count(&glb_json(&serialize_glb(&linear, &options).unwrap())),
            32
        );
        assert_eq!(
            line_segment_count(&glb_json(&serialize_glb(&revolved, &options).unwrap())),
            0
        );
    }

    #[test]
    fn cylinder_edges_are_only_cap_boundaries() {
        let structured = rendered(&Node::Cylinder {
            h: 10.0,
            r1: 4.0,
            r2: 4.0,
            center: false,
            frags: FragmentSpec {
                fn_: 16.0,
                ..FragmentSpec::default()
            },
        });
        let document = glb_json(
            &serialize_glb(
                &structured,
                &Export3DOptions {
                    include_edges: true,
                    ..Export3DOptions::default()
                },
            )
            .unwrap(),
        );

        assert_eq!(line_segment_count(&document), 32);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn rust_and_native_relations_preserve_the_same_boolean_feature_edges() {
        use crate::{render_structured_native_cached, ManifoldKernel};

        let node = Node::Difference(vec![
            Node::Cube {
                size: [4.0, 4.0, 4.0],
                center: false,
            },
            Node::Translate {
                v: [1.0, 1.0, 2.0],
                child: Box::new(Node::Cube {
                    size: [2.0, 2.0, 4.0],
                    center: false,
                }),
            },
        ]);
        let rust = rendered(&node);
        let native =
            render_structured_native_cached(&node, &ManifoldKernel::new(), &mut GeomCache::new())
                .unwrap();
        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };
        let rust_lines = line_segment_count(&glb_json(&serialize_glb(&rust, &options).unwrap()));
        let native_lines =
            line_segment_count(&glb_json(&serialize_glb(&native, &options).unwrap()));

        assert_eq!(rust_lines, 24);
        assert_eq!(native_lines, rust_lines);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn rust_and_native_relations_preserve_source_patch_boundaries() {
        use crate::{render_structured_native_cached, ManifoldKernel};

        let node = Node::Cylinder {
            h: 10.0,
            r1: 4.0,
            r2: 4.0,
            center: false,
            frags: FragmentSpec {
                fn_: 16.0,
                ..FragmentSpec::default()
            },
        };
        let native =
            render_structured_native_cached(&node, &ManifoldKernel::new(), &mut GeomCache::new())
                .unwrap();
        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };

        assert_eq!(
            line_segment_count(&glb_json(&serialize_glb(&native, &options).unwrap())),
            32
        );
    }

    fn extruded(child: Node, height: f64) -> Node {
        Node::LinearExtrude {
            height,
            center: false,
            twist: 0.0,
            scale: [1.0, 1.0],
            slices: 1,
            child: Box::new(child),
        }
    }

    fn edged_line_count(node: &Node) -> u64 {
        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };
        line_segment_count(&glb_json(
            &serialize_glb(&rendered(node), &options).unwrap(),
        ))
    }

    fn circle(radius: f64) -> Node {
        Node::Circle {
            r: radius,
            frags: FragmentSpec {
                fn_: 48.0,
                ..FragmentSpec::default()
            },
        }
    }

    /// A multi-loop profile does not lay its vertices out ring by ring, so
    /// inferring layers from vertex indices files the side wall into the cap
    /// patches and both rims cancel. Four complete rims is the answer.
    #[test]
    fn extruded_annulus_draws_both_rims() {
        let annulus = Node::Difference(vec![circle(10.0), circle(5.0)]);

        assert_eq!(edged_line_count(&extruded(annulus, 4.0)), 192);
    }

    /// The single-loop case must keep classifying through the ring layout,
    /// which is the only way a ruled side wall stays grouped by profile
    /// segment.
    #[test]
    fn extruded_disc_draws_only_its_rims() {
        assert_eq!(edged_line_count(&extruded(circle(10.0), 4.0)), 96);
    }

    /// A genuine polygon keeps every vertical edge: the merge must not treat a
    /// declared corner as tessellation.
    #[test]
    fn extruded_hexagon_keeps_every_vertical_edge() {
        let hexagon = Node::Circle {
            r: 10.0,
            frags: FragmentSpec {
                fn_: 6.0,
                ..FragmentSpec::default()
            },
        };

        assert_eq!(edged_line_count(&extruded(hexagon, 20.0)), 12);
    }

    /// Two patches that meet coplanar are one surface, so a flush union shows
    /// the outer box and no seam across the shared face.
    #[test]
    fn flush_union_draws_no_seam() {
        let stacked = Node::Union(vec![
            Node::Cube {
                size: [20.0, 20.0, 10.0],
                center: false,
            },
            Node::Translate {
                v: [0.0, 0.0, 10.0],
                child: Box::new(Node::Cube {
                    size: [20.0, 20.0, 10.0],
                    center: false,
                }),
            },
        ]);

        assert_eq!(edged_line_count(&stacked), 16);
    }

    /// A hull face lying wholly on one operand inherits that operand's surface,
    /// so the boundary with anything that later cuts it is a patch pair and is
    /// drawn along its whole length rather than eroding toward tangency.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn hull_faces_inherit_their_operand_surface() {
        use crate::{render_structured_native_cached, ManifoldKernel};

        let ball = |x: f64, radius: f64| Node::Translate {
            v: [x, 0.0, 0.0],
            child: Box::new(Node::Sphere {
                r: radius,
                frags: FragmentSpec {
                    fn_: 24.0,
                    ..FragmentSpec::default()
                },
            }),
        };
        let node = Node::Hull(vec![ball(-10.0, 6.0), ball(10.0, 3.0)]);
        let structured =
            render_structured_native_cached(&node, &ManifoldKernel::new(), &mut GeomCache::new())
                .unwrap();

        let patches: BTreeSet<_> = structured.exact.source_face_ids.iter().copied().collect();
        assert!(
            !patches.contains(&crate::structured::UNCLASSIFIED_PATCH_ID),
            "every hull face should resolve to an operand or a ruled band"
        );
        assert_eq!(patches.len(), 3, "two operand surfaces and one ruled band");
    }

    #[test]
    fn one_authored_module_with_two_colors_emits_one_mesh_with_two_primitives() {
        let node = authored(
            "painted_part",
            10,
            Node::Union(vec![
                Node::Color {
                    rgba: [1.0, 0.0, 0.0, 1.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
                Node::Color {
                    rgba: [0.0, 0.0, 1.0, 1.0],
                    child: Box::new(Node::Translate {
                        v: [0.0, 0.0, 2.0],
                        child: Box::new(Node::Cube {
                            size: [1.0, 1.0, 1.0],
                            center: false,
                        }),
                    }),
                },
            ]),
        );
        let document =
            glb_json(&serialize_glb(&rendered(&node), &Export3DOptions::default()).unwrap());

        assert_eq!(document["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(document["nodes"][0]["name"], "Painted Part");
        assert_eq!(document["meshes"].as_array().unwrap().len(), 1);
        assert_eq!(
            document["meshes"][0]["primitives"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn authored_parent_can_own_direct_geometry_and_children_without_duplicate_triangles() {
        let node = authored(
            "assembly",
            10,
            Node::Union(vec![
                Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                },
                authored(
                    "cap",
                    20,
                    Node::Translate {
                        v: [0.0, 0.0, 1.0],
                        child: Box::new(Node::Cube {
                            size: [1.0, 1.0, 1.0],
                            center: false,
                        }),
                    },
                ),
            ]),
        );
        let structured = rendered(&node);
        let document = glb_json(&serialize_glb(&structured, &Export3DOptions::default()).unwrap());

        assert_eq!(document["nodes"][0]["name"], "Assembly");
        assert!(document["nodes"][0]["mesh"].is_number());
        assert_eq!(document["nodes"][0]["children"], json!([1]));
        assert_eq!(document["nodes"][1]["name"], "Cap");
        assert_eq!(
            surface_triangle_count(&document),
            structured.exact.mesh.tris.len() as u64
        );
    }

    #[test]
    fn ownerless_geometry_uses_deterministic_spatial_fallback_names() {
        let document = glb_json(&serialize_glb(&two_cubes(), &Export3DOptions::default()).unwrap());

        assert_eq!(document["scenes"][0]["nodes"], json!([0, 1]));
        assert_eq!(document["nodes"][0]["name"], "#F5A523FF Shape 1");
        assert_eq!(document["nodes"][1]["name"], "#F5A523FF Shape 2");
        assert!(document["nodes"][0]["extras"]["openrscad"]["moduleName"].is_null());
    }

    #[test]
    fn duplicate_authored_siblings_are_source_ordered_while_same_call_site_collapses() {
        let cube = || Node::Cube {
            size: [1.0, 1.0, 1.0],
            center: false,
        };
        let node = authored(
            "assembly",
            10,
            Node::Union(vec![
                authored("bench", 30, cube()),
                authored(
                    "bench",
                    20,
                    Node::Translate {
                        v: [0.0, 0.0, 2.0],
                        child: Box::new(cube()),
                    },
                ),
                authored(
                    "arch_loop",
                    40,
                    Node::Translate {
                        v: [0.0, 0.0, 4.0],
                        child: Box::new(cube()),
                    },
                ),
                authored(
                    "arch_loop",
                    40,
                    Node::Translate {
                        v: [0.0, 0.0, 6.0],
                        child: Box::new(cube()),
                    },
                ),
            ]),
        );
        let document =
            glb_json(&serialize_glb(&rendered(&node), &Export3DOptions::default()).unwrap());
        let children = document["nodes"][0]["children"].as_array().unwrap();
        let names: Vec<_> = children
            .iter()
            .map(|child| {
                document["nodes"][child.as_u64().unwrap() as usize]["name"]
                    .as_str()
                    .unwrap()
            })
            .collect();

        assert_eq!(names, ["Bench 1", "Bench 2", "Arch Loop"]);
        let arch = children[2].as_u64().unwrap() as usize;
        let mesh = document["nodes"][arch]["mesh"].as_u64().unwrap() as usize;
        let accessor = document["meshes"][mesh]["primitives"][0]["indices"]
            .as_u64()
            .unwrap() as usize;
        assert_eq!(document["accessors"][accessor]["count"], 72);
    }

    #[test]
    fn threemf_names_unchanged_physical_solids_from_authored_provenance() {
        let roof = authored(
            "roof_frame",
            20,
            Node::Union(vec![
                Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                },
                Node::Translate {
                    v: [0.0, 0.0, 2.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
            ]),
        );
        let structured = rendered(&roof);
        assert_eq!(structured.solid_components.len(), 2);
        let bytes = serialize_3mf(&structured, &Export3DOptions::default()).unwrap();
        let model_xml = threemf_document(&bytes);
        let model = roxmltree::Document::parse(&model_xml).unwrap();

        assert_eq!(
            threemf_object_names(&model),
            ["Roof Frame 1", "Roof Frame 2"]
        );
        assert_eq!(
            model.root_element().lookup_namespace_uri(Some("openrscad")),
            Some("https://openrscad.com/3mf/metadata/1")
        );
        for object in model
            .descendants()
            .filter(|node| node.has_tag_name("object"))
        {
            let metadata = object
                .descendants()
                .find(|node| {
                    node.has_tag_name("metadata")
                        && node.attribute("name") == Some("openrscad:semanticOwners")
                })
                .unwrap();
            let owners: Value = serde_json::from_str(metadata.text().unwrap()).unwrap();
            assert_eq!(owners[0][0]["moduleName"], "roof_frame");
            assert_eq!(owners[0][0]["callSite"]["start"], 20);
        }
    }

    #[test]
    fn threemf_fused_sibling_modules_use_their_common_authored_owner() {
        let node = authored(
            "assembly",
            10,
            Node::Union(vec![
                authored(
                    "lower_frame",
                    20,
                    Node::Cube {
                        size: [10.0, 10.0, 2.0],
                        center: false,
                    },
                ),
                authored(
                    "roof_frame",
                    30,
                    Node::Translate {
                        v: [0.0, 0.0, 2.0],
                        child: Box::new(Node::Cube {
                            size: [10.0, 10.0, 2.0],
                            center: false,
                        }),
                    },
                ),
            ]),
        );
        let structured = rendered(&node);
        assert_eq!(structured.solid_components.len(), 1);
        let bytes = serialize_3mf(&structured, &Export3DOptions::default()).unwrap();
        let model_xml = threemf_document(&bytes);
        let model = roxmltree::Document::parse(&model_xml).unwrap();

        assert_eq!(threemf_object_names(&model), ["Assembly Solid"]);
    }

    #[test]
    fn threemf_fused_roots_without_common_owner_use_shape_fallback() {
        let node = Node::Union(vec![
            authored(
                "left_part",
                10,
                Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                },
            ),
            authored(
                "right_part",
                20,
                Node::Translate {
                    v: [0.0, 0.0, 1.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
            ),
        ]);
        let structured = rendered(&node);
        assert_eq!(structured.solid_components.len(), 1);
        let model_xml =
            threemf_document(&serialize_3mf(&structured, &Export3DOptions::default()).unwrap());
        let model = roxmltree::Document::parse(&model_xml).unwrap();

        assert_eq!(threemf_object_names(&model), ["Shape"]);
        let owners = model
            .descendants()
            .find(|node| node.attribute("name") == Some("openrscad:semanticOwners"))
            .unwrap();
        let owners: Value = serde_json::from_str(owners.text().unwrap()).unwrap();
        assert_eq!(owners.as_array().unwrap().len(), 2);
    }

    #[test]
    fn structured_3mf_has_one_object_per_spatial_solid() {
        let structured = two_cubes();
        let bytes = serialize_3mf(&structured, &Export3DOptions::default()).unwrap();
        let model = crate::mesh::read_3mf_model(&bytes).unwrap();
        assert_eq!(model.matches("<object ").count(), 2);
        assert_eq!(model.matches("<item ").count(), 2);
        assert_eq!(
            serialize_3mf(&structured, &Export3DOptions::default()).unwrap(),
            bytes
        );
    }

    #[test]
    fn invalid_glb_scale_is_typed_error() {
        let options = Export3DOptions {
            source_unit_to_meters: 0.0,
            ..Export3DOptions::default()
        };
        assert!(serialize_glb(&two_cubes(), &options).is_err());
    }

    #[test]
    fn optional_edges_are_owner_local_deterministic_and_do_no_disabled_work() {
        let structured = two_cubes();
        reset_edge_derivation_count();
        let plain = serialize_glb(&structured, &Export3DOptions::default()).unwrap();
        assert_eq!(edge_derivation_count(), 0);
        let plain = glb_json(&plain);
        assert_eq!(plain["nodes"].as_array().unwrap().len(), 2);
        assert!(plain["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|mesh| mesh["primitives"].as_array().unwrap().len() == 1));

        let options = Export3DOptions {
            include_edges: true,
            ..Export3DOptions::default()
        };
        let edged = serialize_glb(&structured, &options).unwrap();
        assert_eq!(edge_derivation_count(), 2);
        assert_eq!(serialize_glb(&structured, &options).unwrap(), edged);
        let edged = glb_json(&edged);
        assert_eq!(edged["nodes"].as_array().unwrap().len(), 2);
        for mesh in edged["meshes"].as_array().unwrap() {
            let primitives = mesh["primitives"].as_array().unwrap();
            assert_eq!(primitives.len(), 2);
            assert_eq!(primitives[0]["mode"], 4);
            assert_eq!(primitives[1]["mode"], 1);
            let line_accessor = primitives[1]["indices"].as_u64().unwrap() as usize;
            assert_eq!(edged["accessors"][line_accessor]["count"], 24);
        }
    }

    #[test]
    fn multicolor_owner_derives_one_edge_group_after_all_surface_primitives() {
        let node = authored(
            "painted_part",
            10,
            Node::Union(vec![
                Node::Color {
                    rgba: [1.0, 0.0, 0.0, 1.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
                Node::Color {
                    rgba: [0.0, 0.0, 1.0, 1.0],
                    child: Box::new(Node::Translate {
                        v: [0.0, 0.0, 2.0],
                        child: Box::new(Node::Cube {
                            size: [1.0, 1.0, 1.0],
                            center: false,
                        }),
                    }),
                },
            ]),
        );
        reset_edge_derivation_count();
        let document = glb_json(
            &serialize_glb(
                &rendered(&node),
                &Export3DOptions {
                    include_edges: true,
                    ..Export3DOptions::default()
                },
            )
            .unwrap(),
        );

        assert_eq!(edge_derivation_count(), 1);
        assert_eq!(document["nodes"].as_array().unwrap().len(), 1);
        let primitives = document["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(primitives.len(), 3);
        assert_eq!(primitives[0]["mode"], 4);
        assert_eq!(primitives[1]["mode"], 4);
        assert_eq!(primitives[2]["mode"], 1);
    }
}
