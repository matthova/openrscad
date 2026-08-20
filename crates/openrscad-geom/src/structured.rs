//! Exact attributed geometry used by structured 3D exporters.

use super::{
    hash_all, is_2d, mirror, mult_matrix, render_node, rotate, scale, translate, Ctx, DisplayMode,
    GeomCache, GeomError, Kernel, Mesh, RenderMode, DEFAULT_COLOR,
};
use openrscad_ir::{Node, ProvenanceFrame};
use std::collections::{BTreeMap, HashMap, HashSet};

use std::cell::RefCell;
#[cfg(feature = "benchmark-profile")]
use web_time::Instant;

#[cfg(feature = "benchmark-profile")]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BenchmarkProfile {
    pub attributed_render_ms: f64,
    pub boolean_ms: f64,
    pub partition_ms: f64,
    pub edge_derivation_ms: f64,
    pub feature_line_count: usize,
}

#[cfg(feature = "benchmark-profile")]
thread_local! {
    static BENCHMARK_PROFILE: RefCell<BenchmarkProfile> = RefCell::new(BenchmarkProfile::default());
}

#[cfg(feature = "benchmark-profile")]
pub fn reset_benchmark_profile() {
    BENCHMARK_PROFILE.with(|profile| *profile.borrow_mut() = BenchmarkProfile::default());
}

#[cfg(feature = "benchmark-profile")]
pub fn take_benchmark_profile() -> BenchmarkProfile {
    BENCHMARK_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

#[cfg(feature = "benchmark-profile")]
pub(crate) fn record_edge_derivation(duration_ms: f64, line_count: usize) {
    BENCHMARK_PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        profile.edge_derivation_ms += duration_ms;
        profile.feature_line_count += line_count;
    });
}

#[cfg(feature = "benchmark-profile")]
fn record_boolean<T>(operation: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = operation();
    BENCHMARK_PROFILE.with(|profile| {
        profile.borrow_mut().boolean_ms += started.elapsed().as_secs_f64() * 1_000.0;
    });
    result
}

#[cfg(not(feature = "benchmark-profile"))]
fn record_boolean<T>(operation: impl FnOnce() -> T) -> T {
    operation()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SurfaceAttributionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ContributionKind {
    Boundary,
    DifferenceTool,
    IntersectionOperand,
    GeneratedOpaqueOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AttributionStatus {
    Exact,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SurfaceAttribution {
    pub rgba: [f32; 4],
    pub mode: DisplayMode,
    pub provenance: Vec<ProvenanceFrame>,
    pub contributors: Vec<Vec<ProvenanceFrame>>,
    pub contribution: ContributionKind,
    pub status: AttributionStatus,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AttributedMesh {
    pub mesh: Mesh,
    pub surface_ids: Vec<SurfaceAttributionId>,
    pub source_face_ids: Vec<u64>,
}

impl AttributedMesh {
    fn validate(&self) -> Result<(), GeomError> {
        let triangles = self.mesh.tris.len();
        if triangles == 0 {
            if self.surface_ids.is_empty() && self.source_face_ids.is_empty() {
                return Ok(());
            }
            return Err(GeomError::Invariant(
                "empty mesh has non-empty triangle attribution".to_string(),
            ));
        }
        if self.surface_ids.len() != triangles {
            return Err(GeomError::Invariant(format!(
                "{} triangles but {} surface IDs",
                triangles,
                self.surface_ids.len()
            )));
        }
        if self.source_face_ids.len() != triangles {
            return Err(GeomError::Invariant(format!(
                "{} triangles but {} source face IDs",
                triangles,
                self.source_face_ids.len()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshSelection {
    pub triangles: Vec<u32>,
    pub bounds: Bounds3,
    pub provenance: Vec<ProvenanceFrame>,
    pub attribution: AttributionStatus,
    pub geometry_hash: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredMesh {
    /// Complete authored-scene geometry; structural siblings are not fused.
    pub(crate) exact: AttributedMesh,
    /// Ordinary exact render used for aggregate measurements and fused exports.
    pub(crate) aggregate: Mesh,
    pub(crate) include_display_modes: bool,
    pub(crate) solid_components: Vec<MeshSelection>,
    pub(crate) attributions: Vec<SurfaceAttribution>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct SurfaceKey {
    rgba: [u32; 4],
    mode: DisplayMode,
    provenance: Vec<ProvenanceFrame>,
    contributors: Vec<Vec<ProvenanceFrame>>,
    contribution: ContributionKind,
    status: AttributionStatus,
}

#[derive(Default)]
struct SurfaceTable {
    values: Vec<SurfaceAttribution>,
    ids: HashMap<SurfaceKey, SurfaceAttributionId>,
}

impl SurfaceTable {
    fn intern(&mut self, value: SurfaceAttribution) -> SurfaceAttributionId {
        let key = SurfaceKey {
            rgba: value.rgba.map(f32::to_bits),
            mode: value.mode,
            provenance: value.provenance.clone(),
            contributors: value.contributors.clone(),
            contribution: value.contribution,
            status: value.status,
        };
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = SurfaceAttributionId(self.values.len() as u32);
        self.values.push(value);
        self.ids.insert(key, id);
        id
    }
}

#[derive(Clone)]
struct ActiveSurface {
    rgba: [f32; 4],
    color_explicit: bool,
    mode: DisplayMode,
    provenance: Vec<ProvenanceFrame>,
}

impl Default for ActiveSurface {
    fn default() -> Self {
        Self {
            rgba: DEFAULT_COLOR,
            color_explicit: false,
            mode: DisplayMode::Solid,
            provenance: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct RelationMesh {
    attributed: AttributedMesh,
    backside: Vec<bool>,
}

impl RelationMesh {
    fn empty() -> Self {
        Self {
            attributed: AttributedMesh::default(),
            backside: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), GeomError> {
        self.attributed.validate()?;
        if self.backside.len() != self.attributed.mesh.tris.len() {
            return Err(GeomError::Invariant(format!(
                "{} triangles but {} backside flags",
                self.attributed.mesh.tris.len(),
                self.backside.len()
            )));
        }
        Ok(())
    }
}

trait RelationKernel {
    fn union(&self, meshes: Vec<RelationMesh>, keys: &RunKeys) -> Result<RelationMesh, GeomError>;
    fn difference(
        &self,
        base: RelationMesh,
        tools: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError>;
    fn intersection(
        &self,
        meshes: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError>;
}

#[derive(Default)]
pub(crate) struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    pub(crate) fn find(&mut self, value: usize) -> usize {
        if self.parent[value] != value {
            self.parent[value] = self.find(self.parent[value]);
        }
        self.parent[value]
    }

    pub(crate) fn union(&mut self, left: usize, right: usize) {
        let mut left = self.find(left);
        let mut right = self.find(right);
        if left == right {
            return;
        }
        if self.rank[left] < self.rank[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left;
        if self.rank[left] == self.rank[right] {
            self.rank[left] += 1;
        }
    }
}

fn selection_bounds(mesh: &Mesh, triangles: &[u32]) -> Bounds3 {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for triangle in triangles {
        for vertex in mesh.tris[*triangle as usize] {
            let point = mesh.verts[vertex as usize];
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
    }
    Bounds3 { min, max }
}

fn canonical_geometry_hash(mesh: &Mesh, triangles: &[u32]) -> u64 {
    let mut canonical = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        let mut points = mesh.tris[*triangle as usize]
            .map(|vertex| mesh.verts[vertex as usize].map(f64::to_bits));
        points.sort_unstable();
        canonical.push(points);
    }
    canonical.sort_unstable();
    let mut hash = 0xcbf29ce484222325u64;
    for triangle in canonical {
        for point in triangle {
            for component in point {
                for byte in component.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
        }
    }
    hash
}

pub(crate) fn make_selection(
    exact: &AttributedMesh,
    attributions: &[SurfaceAttribution],
    mut triangles: Vec<u32>,
) -> MeshSelection {
    triangles.sort_unstable();
    let mut provenance = Vec::new();
    let mut status = AttributionStatus::Exact;
    for triangle in &triangles {
        let surface = &attributions[exact.surface_ids[*triangle as usize].0 as usize];
        if surface.status == AttributionStatus::Ambiguous {
            status = AttributionStatus::Ambiguous;
        }
        for frame in &surface.provenance {
            if !provenance.contains(frame) {
                provenance.push(frame.clone());
            }
        }
    }
    MeshSelection {
        bounds: selection_bounds(&exact.mesh, &triangles),
        geometry_hash: canonical_geometry_hash(&exact.mesh, &triangles),
        triangles,
        provenance,
        attribution: status,
    }
}

pub(crate) fn spatial_order(left: &MeshSelection, right: &MeshSelection) -> std::cmp::Ordering {
    left.bounds.min[2]
        .total_cmp(&right.bounds.min[2])
        .then_with(|| left.bounds.min[0].total_cmp(&right.bounds.min[0]))
        .then_with(|| left.bounds.min[1].total_cmp(&right.bounds.min[1]))
        .then_with(|| left.bounds.max[2].total_cmp(&right.bounds.max[2]))
        .then_with(|| left.bounds.max[0].total_cmp(&right.bounds.max[0]))
        .then_with(|| left.bounds.max[1].total_cmp(&right.bounds.max[1]))
        .then_with(|| left.triangles.len().cmp(&right.triangles.len()))
        .then_with(|| left.geometry_hash.cmp(&right.geometry_hash))
}

pub(crate) fn components_for_triangles(mesh: &Mesh, triangles: &[u32]) -> Vec<Vec<u32>> {
    let mut union = UnionFind::new(mesh.tris.len());
    let mut first_by_vertex: HashMap<u32, usize> = HashMap::new();
    for triangle in triangles {
        let triangle_index = *triangle as usize;
        for vertex in mesh.tris[triangle_index] {
            if let Some(first) = first_by_vertex.insert(vertex, triangle_index) {
                union.union(first, triangle_index);
            }
        }
    }
    let mut components: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for triangle in triangles {
        components
            .entry(union.find(*triangle as usize))
            .or_default()
            .push(*triangle);
    }
    components.into_values().collect()
}

pub(super) fn partition(
    exact: AttributedMesh,
    aggregate: Mesh,
    include_display_modes: bool,
    attributions: Vec<SurfaceAttribution>,
) -> Result<StructuredMesh, GeomError> {
    exact.validate()?;
    if exact.mesh.tris.is_empty() {
        return Ok(StructuredMesh {
            exact,
            aggregate,
            include_display_modes,
            solid_components: Vec::new(),
            attributions,
        });
    }
    if exact
        .surface_ids
        .iter()
        .any(|id| id.0 as usize >= attributions.len())
    {
        return Err(GeomError::Invariant(
            "surface ID does not resolve in the attribution table".to_string(),
        ));
    }

    let all: Vec<u32> = (0..exact.mesh.tris.len() as u32).collect();
    let mut solid_components: Vec<_> = components_for_triangles(&exact.mesh, &all)
        .into_iter()
        .map(|triangles| make_selection(&exact, &attributions, triangles))
        .collect();

    solid_components.sort_by(spatial_order);
    Ok(StructuredMesh {
        exact,
        aggregate,
        include_display_modes,
        solid_components,
        attributions,
    })
}

/// Interns `(surface, source patch)` pairs into the `originalID` values Manifold
/// propagates through booleans, so both operands of a relation agree on what a
/// run means and the pair can be recovered from the result.
///
/// Manifold's own `faceID` cannot serve this purpose: it groups coplanar
/// triangles and splits smooth ones, and the native `MeshGL64` binding has no
/// faceID input at all. The run channel is propagated by both relation kernels.
#[derive(Default)]
pub(crate) struct RunKeys {
    ids: RefCell<HashMap<(u32, u64), u32>>,
    pairs: RefCell<Vec<(SurfaceAttributionId, u64)>>,
}

impl RunKeys {
    fn intern(&self, surface: SurfaceAttributionId, patch: u64) -> Result<u32, GeomError> {
        let mut ids = self.ids.borrow_mut();
        if let Some(id) = ids.get(&(surface.0, patch)) {
            return Ok(*id);
        }
        let mut pairs = self.pairs.borrow_mut();
        pairs.push((surface, patch));
        let id = u32::try_from(pairs.len())
            .map_err(|_| GeomError::Invariant("relation run ID overflow".to_string()))?;
        ids.insert((surface.0, patch), id);
        Ok(id)
    }

    fn decode(&self, original_id: u32) -> Option<(SurfaceAttributionId, u64)> {
        let index = usize::try_from(original_id).ok()?.checked_sub(1)?;
        self.pairs.borrow().get(index).copied()
    }
}

struct RelationBuffers {
    vertices: Vec<f64>,
    triangles: Vec<u64>,
    merge_from_vert: Vec<u64>,
    merge_to_vert: Vec<u64>,
    run_index: Vec<u64>,
    run_original_id: Vec<u32>,
}

fn relation_buffers(mesh: &RelationMesh, keys: &RunKeys) -> Result<RelationBuffers, GeomError> {
    mesh.validate()?;
    let mut records = Vec::with_capacity(mesh.attributed.mesh.tris.len());
    for (triangle_index, triangle) in mesh.attributed.mesh.tris.iter().enumerate() {
        records.push((
            mesh.attributed.surface_ids[triangle_index],
            mesh.attributed.source_face_ids[triangle_index],
            *triangle,
        ));
    }
    records.sort_by_key(|(surface, patch, _)| (surface.0, *patch));

    let mut triangles = Vec::with_capacity(records.len() * 3);
    let mut run_index = Vec::new();
    let mut run_original_id = Vec::new();
    let mut previous = None;
    for (triangle_index, (surface, patch, triangle)) in records.into_iter().enumerate() {
        if previous != Some((surface, patch)) {
            run_index.push((triangle_index * 3) as u64);
            run_original_id.push(keys.intern(surface, patch)?);
            previous = Some((surface, patch));
        }
        triangles.extend(triangle.map(u64::from));
    }
    if !run_original_id.is_empty() {
        run_index.push(triangles.len() as u64);
    }
    let vertices = mesh
        .attributed
        .mesh
        .verts
        .iter()
        .flatten()
        .copied()
        .collect();
    Ok(RelationBuffers {
        vertices,
        triangles,
        merge_from_vert: Vec::new(),
        merge_to_vert: Vec::new(),
        run_index,
        run_original_id,
    })
}

struct RelationParts<'a> {
    num_prop: usize,
    vertex_properties: &'a [f64],
    triangle_vertices: &'a [u64],
    run_index: &'a [u64],
    run_original_id: &'a [u32],
    merge_from_vert: &'a [u64],
    merge_to_vert: &'a [u64],

    run_flags: &'a [u8],
}

fn relation_from_parts(
    parts: RelationParts<'_>,
    keys: &RunKeys,
) -> Result<RelationMesh, GeomError> {
    let RelationParts {
        num_prop,
        vertex_properties,
        triangle_vertices,
        run_index,
        run_original_id,
        merge_from_vert,
        merge_to_vert,

        run_flags,
    } = parts;
    if num_prop < 3 || vertex_properties.len() % num_prop != 0 {
        return Err(GeomError::Invariant(
            "relation mesh has invalid vertex properties".to_string(),
        ));
    }
    if triangle_vertices.len() % 3 != 0 {
        return Err(GeomError::Invariant(
            "relation mesh has invalid triangle indices".to_string(),
        ));
    }
    let triangle_count = triangle_vertices.len() / 3;
    let vertex_count = vertex_properties.len() / num_prop;
    if merge_from_vert.len() != merge_to_vert.len() {
        return Err(GeomError::Invariant(
            "relation merge vectors have different lengths".to_string(),
        ));
    }
    if run_original_id.is_empty() && triangle_count != 0 {
        return Err(GeomError::Invariant(
            "non-empty relation mesh has no runs".to_string(),
        ));
    }
    if !run_original_id.is_empty() && run_index.len() != run_original_id.len() + 1 {
        return Err(GeomError::Invariant(format!(
            "{} relation runs but {} run boundaries",
            run_original_id.len(),
            run_index.len()
        )));
    }

    let verts = vertex_properties
        .chunks(num_prop)
        .map(|properties| [properties[0], properties[1], properties[2]])
        .collect();
    let mut merged: Vec<u64> = (0..vertex_count as u64).collect();
    for (&from, &to) in merge_from_vert.iter().zip(merge_to_vert) {
        if from as usize >= vertex_count || to as usize >= vertex_count {
            return Err(GeomError::Invariant(
                "relation merge vertex is outside the vertex buffer".to_string(),
            ));
        }
        merged[from as usize] = to;
    }
    for vertex in 0..merged.len() {
        let mut target = merged[vertex];
        let mut resolved = false;
        for _ in 0..merged.len() {
            let next = merged[target as usize];
            if next == target {
                merged[vertex] = target;
                resolved = true;
                break;
            }
            target = next;
        }
        if !resolved {
            return Err(GeomError::Invariant(
                "relation merge metadata contains a cycle".to_string(),
            ));
        }
    }
    let tris = triangle_vertices
        .chunks(3)
        .map(|triangle| {
            let mut mapped = [0u32; 3];
            for (index, vertex) in triangle.iter().enumerate() {
                if *vertex as usize >= vertex_count {
                    return Err(GeomError::Invariant(
                        "relation triangle vertex is outside the vertex buffer".to_string(),
                    ));
                }
                mapped[index] = u32::try_from(merged[*vertex as usize]).map_err(|_| {
                    GeomError::Invariant("relation vertex index exceeds u32".to_string())
                })?;
            }
            Ok(mapped)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut surface_ids = vec![SurfaceAttributionId(0); triangle_count];
    let mut source_face_ids = vec![0u64; triangle_count];
    let mut backside = vec![false; triangle_count];
    for (run, original_id) in run_original_id.iter().enumerate() {
        let (surface_id, patch) = keys.decode(*original_id).ok_or_else(|| {
            GeomError::Invariant(format!("relation run has unknown ID {original_id}"))
        })?;
        let start = run_index[run] as usize / 3;
        let end = run_index[run + 1] as usize / 3;
        if start > end || end > triangle_count {
            return Err(GeomError::Invariant(
                "relation run is outside the triangle buffer".to_string(),
            ));
        }
        for triangle in start..end {
            surface_ids[triangle] = surface_id;
            source_face_ids[triangle] = patch;
            backside[triangle] = run_flags.get(run).copied().unwrap_or(0) & 1 != 0;
        }
    }
    let result = RelationMesh {
        attributed: AttributedMesh {
            mesh: Mesh { verts, tris },
            surface_ids,
            source_face_ids,
        },
        backside,
    };
    result.validate()?;
    Ok(result)
}

fn balanced_relation<T>(
    mut values: Vec<T>,
    combine: impl Fn(&T, &T) -> Result<T, GeomError>,
) -> Result<Option<T>, GeomError> {
    if values.is_empty() {
        return Ok(None);
    }
    while values.len() > 1 {
        let mut next = Vec::with_capacity(values.len().div_ceil(2));
        let mut iter = values.into_iter();
        while let Some(left) = iter.next() {
            match iter.next() {
                Some(right) => next.push(combine(&left, &right)?),
                None => next.push(left),
            }
        }
        values = next;
    }
    Ok(values.pop())
}

#[cfg(any(target_arch = "wasm32", test))]
struct RustRelationKernel;

#[cfg(any(target_arch = "wasm32", test))]
impl RustRelationKernel {
    fn to_manifold(
        mesh: &RelationMesh,
        keys: &RunKeys,
    ) -> Result<manifold_rust::manifold::Manifold, GeomError> {
        use manifold_rust::types::MeshGL64;

        let buffers = relation_buffers(mesh, keys)?;
        let mesh = MeshGL64 {
            num_prop: 3,
            vert_properties: buffers.vertices,
            tri_verts: buffers.triangles,
            merge_from_vert: buffers.merge_from_vert,
            merge_to_vert: buffers.merge_to_vert,
            run_index: buffers.run_index,
            run_original_id: buffers.run_original_id,
            ..MeshGL64::default()
        };
        let manifold = manifold_rust::manifold::Manifold::from_mesh_gl64(&mesh);
        match manifold.status() {
            manifold_rust::types::Error::NoError => Ok(manifold),
            status => Err(GeomError::Kernel(status.to_string())),
        }
    }

    fn from_manifold(
        manifold: &manifold_rust::manifold::Manifold,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        let mesh = manifold.get_mesh_gl64(-1);
        relation_from_parts(
            RelationParts {
                num_prop: mesh.num_prop as usize,
                vertex_properties: &mesh.vert_properties,
                triangle_vertices: &mesh.tri_verts,
                run_index: &mesh.run_index,
                run_original_id: &mesh.run_original_id,
                merge_from_vert: &mesh.merge_from_vert,
                merge_to_vert: &mesh.merge_to_vert,
                run_flags: &mesh.run_flags,
            },
            keys,
        )
    }

    fn checked(
        manifold: manifold_rust::manifold::Manifold,
    ) -> Result<manifold_rust::manifold::Manifold, GeomError> {
        match manifold.status() {
            manifold_rust::types::Error::NoError => Ok(manifold),
            status => Err(GeomError::Kernel(status.to_string())),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl RelationKernel for RustRelationKernel {
    fn union(&self, meshes: Vec<RelationMesh>, keys: &RunKeys) -> Result<RelationMesh, GeomError> {
        let manifolds = meshes
            .iter()
            .filter(|mesh| !mesh.attributed.mesh.is_empty())
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = balanced_relation(manifolds, |left, right| Self::checked(left.union(right)))?;
        result
            .as_ref()
            .map(|manifold| Self::from_manifold(manifold, keys))
            .unwrap_or_else(|| Ok(RelationMesh::empty()))
    }

    fn difference(
        &self,
        base: RelationMesh,
        tools: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        if base.attributed.mesh.is_empty() {
            return Ok(RelationMesh::empty());
        }
        let base = Self::to_manifold(&base, keys)?;
        let tools = tools
            .iter()
            .filter(|mesh| !mesh.attributed.mesh.is_empty())
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = match balanced_relation(tools, |left, right| Self::checked(left.union(right)))?
        {
            Some(tools) => Self::checked(base.difference(&tools))?,
            None => base,
        };
        Self::from_manifold(&result, keys)
    }

    fn intersection(
        &self,
        meshes: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        if meshes.is_empty() || meshes.iter().any(|mesh| mesh.attributed.mesh.is_empty()) {
            return Ok(RelationMesh::empty());
        }
        let manifolds = meshes
            .iter()
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = balanced_relation(manifolds, |left, right| {
            Self::checked(left.intersection(right))
        })?;
        Self::from_manifold(&result.expect("non-empty relation intersection"), keys)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct NativeRelationKernel;

#[cfg(not(target_arch = "wasm32"))]
impl NativeRelationKernel {
    fn to_manifold(
        mesh: &RelationMesh,
        keys: &RunKeys,
    ) -> Result<manifold_csg::Manifold, GeomError> {
        use manifold_csg::{MeshGL64, MeshGL64Options};

        let buffers = relation_buffers(mesh, keys)?;
        let options = MeshGL64Options::new()
            .runs(&buffers.run_index, &buffers.run_original_id)
            .merge_vertices(&buffers.merge_from_vert, &buffers.merge_to_vert);
        let mesh = MeshGL64::new_with_options(&buffers.vertices, 3, &buffers.triangles, options)
            .map_err(|error| GeomError::Kernel(format!("MeshGL64 relation input: {error:?}")))?;
        let manifold = manifold_csg::Manifold::from_meshgl64(&mesh)
            .map_err(|error| GeomError::Kernel(format!("relation input: {error:?}")))?;
        manifold
            .status()
            .map_err(|error| GeomError::NonManifold(format!("{error:?}")))?;
        Ok(manifold)
    }

    fn from_manifold(
        manifold: &manifold_csg::Manifold,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        let mesh = manifold.to_meshgl64();
        relation_from_parts(
            RelationParts {
                num_prop: mesh.num_prop(),
                vertex_properties: &mesh.vert_properties(),
                triangle_vertices: &mesh.tri_verts(),
                run_index: &mesh.run_index(),
                run_original_id: &mesh.run_original_id(),
                merge_from_vert: &mesh.merge_from_vert(),
                merge_to_vert: &mesh.merge_to_vert(),
                run_flags: &mesh.run_flags(),
            },
            keys,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RelationKernel for NativeRelationKernel {
    fn union(&self, meshes: Vec<RelationMesh>, keys: &RunKeys) -> Result<RelationMesh, GeomError> {
        let manifolds = meshes
            .iter()
            .filter(|mesh| !mesh.attributed.mesh.is_empty())
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = balanced_relation(manifolds, |left, right| Ok(left + right))?;
        result
            .as_ref()
            .map(|manifold| Self::from_manifold(manifold, keys))
            .unwrap_or_else(|| Ok(RelationMesh::empty()))
    }

    fn difference(
        &self,
        base: RelationMesh,
        tools: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        if base.attributed.mesh.is_empty() {
            return Ok(RelationMesh::empty());
        }
        let base = Self::to_manifold(&base, keys)?;
        let tools = tools
            .iter()
            .filter(|mesh| !mesh.attributed.mesh.is_empty())
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = match balanced_relation(tools, |left, right| Ok(left + right))? {
            Some(tools) => &base - &tools,
            None => base,
        };
        Self::from_manifold(&result, keys)
    }

    fn intersection(
        &self,
        meshes: Vec<RelationMesh>,
        keys: &RunKeys,
    ) -> Result<RelationMesh, GeomError> {
        if meshes.is_empty() || meshes.iter().any(|mesh| mesh.attributed.mesh.is_empty()) {
            return Ok(RelationMesh::empty());
        }
        let manifolds = meshes
            .iter()
            .map(|mesh| Self::to_manifold(mesh, keys))
            .collect::<Result<Vec<_>, _>>()?;
        let result = balanced_relation(manifolds, |left, right| Ok(left ^ right))?
            .expect("non-empty relation intersection");
        Self::from_manifold(&result, keys)
    }
}

struct DetailedCtx<'a, 'b> {
    ordinary: &'a mut Ctx<'b>,
    relation: &'a dyn RelationKernel,
    surfaces: SurfaceTable,
    include_backgrounds: bool,
    next_patch_id: u64,
    run_keys: RunKeys,
}

impl DetailedCtx<'_, '_> {
    fn global_patch_ids(&mut self, local: Vec<u64>) -> Vec<u64> {
        let mut ids = HashMap::new();
        local
            .into_iter()
            .map(|patch| {
                if patch == UNCLASSIFIED_PATCH_ID {
                    return patch;
                }
                *ids.entry(patch).or_insert_with(|| {
                    let id = self.next_patch_id;
                    self.next_patch_id += 1;
                    id
                })
            })
            .collect()
    }

    fn active_id(
        &mut self,
        active: &ActiveSurface,
        contribution: ContributionKind,
        status: AttributionStatus,
    ) -> SurfaceAttributionId {
        self.surfaces.intern(SurfaceAttribution {
            rgba: active.rgba,
            mode: active.mode,
            provenance: active.provenance.clone(),
            contributors: Vec::new(),
            contribution,
            status,
        })
    }

    fn generated_id(
        &mut self,
        appearance_id: SurfaceAttributionId,
        surface_ids: &HashSet<SurfaceAttributionId>,
        active: &ActiveSurface,
        contribution: ContributionKind,
    ) -> SurfaceAttributionId {
        let mut ids: Vec<_> = surface_ids.iter().copied().collect();
        ids.sort_unstable();
        let appearance = self
            .surfaces
            .values
            .get(appearance_id.0 as usize)
            .cloned()
            .expect("generated surface requires an appearance");
        let mut contributors: Vec<Vec<ProvenanceFrame>> = ids
            .iter()
            .flat_map(|id| {
                let value = &self.surfaces.values[id.0 as usize];
                if value.contributors.is_empty() {
                    vec![authored_path(&value.provenance)]
                } else {
                    value.contributors.clone()
                }
            })
            .filter(|path| !path.is_empty())
            .collect();
        contributors.sort_by(|left, right| compare_provenance_paths(left, right));
        contributors.dedup();
        let active_owner = authored_path(&active.provenance);
        let provenance = if active_owner.is_empty() {
            longest_common_prefix(&contributors)
        } else {
            active_owner
        };
        let status = if contributors.len() == 1 {
            AttributionStatus::Exact
        } else {
            AttributionStatus::Ambiguous
        };

        self.surfaces.intern(SurfaceAttribution {
            rgba: appearance.rgba,
            mode: appearance.mode,
            provenance,
            contributors,
            contribution,
            status,
        })
    }
}

pub(crate) fn authored_path(frames: &[ProvenanceFrame]) -> Vec<ProvenanceFrame> {
    frames
        .iter()
        .filter(|frame| frame.module_name.is_some())
        .cloned()
        .collect()
}

pub(crate) fn compare_provenance_paths(
    left: &[ProvenanceFrame],
    right: &[ProvenanceFrame],
) -> std::cmp::Ordering {
    let key = |frame: &ProvenanceFrame| {
        (
            frame.call_site.source_id,
            frame.call_site.start,
            frame.call_site.end,
            frame
                .definition_site
                .as_ref()
                .map(|span| (span.source_id, span.start, span.end)),
            frame.module_name.clone(),
        )
    };
    left.iter().map(key).cmp(right.iter().map(key))
}

pub(crate) fn longest_common_prefix(paths: &[Vec<ProvenanceFrame>]) -> Vec<ProvenanceFrame> {
    let Some(first) = paths.first() else {
        return Vec::new();
    };
    let length = paths.iter().skip(1).fold(first.len(), |length, path| {
        length.min(
            first
                .iter()
                .zip(path)
                .take_while(|(left, right)| left == right)
                .count(),
        )
    });
    first[..length].to_vec()
}

pub(crate) const UNCLASSIFIED_PATCH_ID: u64 = u64::MAX;

fn intern_patch<K: Ord>(patches: &mut BTreeMap<K, u64>, key: K) -> u64 {
    let next = patches.len() as u64;
    *patches.entry(key).or_insert(next)
}

#[derive(Clone, Copy)]
enum ProfileBoundary {
    Smooth,
    Polygonal,
    Unknown,
}

/// The boundary kind of a combined profile: identical children keep their kind,
/// anything mixed or unrecognised is unknown.
fn combined_boundary<'a>(children: impl IntoIterator<Item = &'a Node>) -> ProfileBoundary {
    let mut combined = None;
    for child in children {
        let boundary = profile_boundary(child);
        combined = match (combined, boundary) {
            (None, kind) => Some(kind),
            (Some(ProfileBoundary::Smooth), ProfileBoundary::Smooth) => {
                Some(ProfileBoundary::Smooth)
            }
            (Some(ProfileBoundary::Polygonal), ProfileBoundary::Polygonal) => {
                Some(ProfileBoundary::Polygonal)
            }
            _ => return ProfileBoundary::Unknown,
        };
    }
    combined.unwrap_or(ProfileBoundary::Unknown)
}

fn profile_boundary(node: &Node) -> ProfileBoundary {
    match node {
        Node::Circle { .. } => ProfileBoundary::Smooth,
        Node::Square { .. } | Node::Polygon { .. } => ProfileBoundary::Polygonal,
        Node::Translate { child, .. }
        | Node::Rotate { child, .. }
        | Node::Scale { child, .. }
        | Node::Mirror { child, .. }
        | Node::MultMatrix { child, .. }
        | Node::Resize { child, .. }
        | Node::Color { child, .. }
        | Node::Highlight(child)
        | Node::Background(child)
        | Node::Provenance { child, .. } => profile_boundary(child),
        // A 2D combination keeps its operands' boundary kind: the outline of a
        // difference of circles is still made of arcs, and of squares still of
        // straight runs. Without this the whole side wall of the extrusion goes
        // unclassified and its rims fall back to per-edge crease detection.
        Node::Union(children)
        | Node::Difference(children)
        | Node::Intersection(children)
        | Node::Hull(children)
        | Node::Minkowski(children)
        | Node::Group(children) => combined_boundary(children),
        // A rounding offset replaces corners with arcs; a straight offset keeps
        // the child's outline kind.
        Node::Offset { r, child, .. } => {
            if *r != 0.0 {
                ProfileBoundary::Smooth
            } else {
                profile_boundary(child)
            }
        }
        _ => ProfileBoundary::Unknown,
    }
}

/// Groups connected triangles that share a plane into one patch.
///
/// Used for extrusion side walls whose profile is polygonal: each straight run
/// of the profile sweeps one planar face, and grouping by coplanarity recovers
/// those faces without assuming anything about how the tessellator ordered its
/// vertices.
fn coplanar_groups(mesh: &Mesh, triangles: &[usize]) -> HashMap<usize, u64> {
    let index_of: HashMap<_, _> = triangles
        .iter()
        .enumerate()
        .map(|(slot, triangle)| (*triangle, slot))
        .collect();
    let mut union = UnionFind::new(triangles.len());
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in triangles {
        let corners = mesh.tris[*triangle];
        for edge in [
            [corners[0], corners[1]],
            [corners[1], corners[2]],
            [corners[2], corners[0]],
        ] {
            let key = (edge[0].min(edge[1]), edge[0].max(edge[1]));
            match edges.entry(key) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(*triangle);
                }
                std::collections::hash_map::Entry::Occupied(slot) => {
                    let other = *slot.get();
                    // 1e-9 rad ~ 6e-8 degrees: coincident planes only, never a
                    // smoothing threshold. Real creases are handled downstream.
                    if dihedral_between(mesh, other, *triangle) < 1e-9 {
                        union.union(index_of[&other], index_of[triangle]);
                    }
                }
            }
        }
    }
    triangles
        .iter()
        .enumerate()
        .map(|(slot, triangle)| (*triangle, union.find(slot) as u64))
        .collect()
}

fn dihedral_between(mesh: &Mesh, left: usize, right: usize) -> f64 {
    let normal_of = |triangle: usize| {
        let corners = mesh.tris[triangle];
        let a = mesh.verts[corners[0] as usize];
        let b = mesh.verts[corners[1] as usize];
        let c = mesh.verts[corners[2] as usize];
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if length > 0.0 {
            [n[0] / length, n[1] / length, n[2] / length]
        } else {
            [0.0, 0.0, 0.0]
        }
    };
    let (left, right) = (normal_of(left), normal_of(right));
    let cosine = (left[0] * right[0] + left[1] * right[1] + left[2] * right[2]).clamp(-1.0, 1.0);
    cosine.acos()
}

/// Confirms that the mesh really is laid out ring by ring, returning the ring
/// size when it is.
///
/// The extrusion tessellator emits `slices + 1` rings of equal size for a
/// single-loop profile, which lets the profile index be recovered as
/// `vertex % ring_size` — the only way to keep a *ruled* side wall (twisted or
/// scaled) grouped by profile segment. A multi-loop profile does not honour
/// that layout, so the assumption has to be checked rather than taken.
fn validated_ring_layout(mesh: &Mesh, slices: u32) -> Option<usize> {
    let ring_count = slices.max(1) as usize + 1;
    if mesh.verts.is_empty() || mesh.verts.len() % ring_count != 0 {
        return None;
    }
    let ring_size = mesh.verts.len() / ring_count;
    if ring_size == 0 {
        return None;
    }
    let (min, max) = mesh.bbox()?;
    let tolerance = (max[2] - min[2]).abs() * 1e-9 + f64::EPSILON;
    let mut previous = None;
    for ring in 0..ring_count {
        let base = mesh.verts[ring * ring_size][2];
        if !mesh.verts[ring * ring_size..(ring + 1) * ring_size]
            .iter()
            .all(|vertex| (vertex[2] - base).abs() <= tolerance)
        {
            return None;
        }
        if previous.is_some_and(|last: f64| base <= last) {
            return None;
        }
        previous = Some(base);
    }
    Some(ring_size)
}

/// Classifies a linear extrusion into bottom cap, top cap and side wall.
///
/// Layer membership comes from the vertex z ordinate rather than from index
/// arithmetic over a presumed ring layout: a multi-loop profile — anything with
/// a hole — does not lay its vertices out ring by ring, and inferring layers
/// from indices files most of the side wall into the cap patches, which cancels
/// the rims.
fn linear_extrude_patch_ids(mesh: &Mesh, slices: u32, boundary: ProfileBoundary) -> Vec<u64> {
    if mesh.tris.is_empty() {
        return Vec::new();
    }
    let Some((min, max)) = mesh.bbox() else {
        return vec![UNCLASSIFIED_PATCH_ID; mesh.tris.len()];
    };
    let tolerance = (max[2] - min[2]).abs() * 1e-9 + f64::EPSILON;
    let at = |value: f64, plane: f64| (value - plane).abs() <= tolerance;

    let mut walls = Vec::new();
    let mut kinds = Vec::with_capacity(mesh.tris.len());
    for (index, triangle) in mesh.tris.iter().enumerate() {
        let z = triangle.map(|vertex| mesh.verts[vertex as usize][2]);
        if z.iter().all(|value| at(*value, min[2])) {
            kinds.push(0u8);
        } else if z.iter().all(|value| at(*value, max[2])) {
            kinds.push(1u8);
        } else {
            kinds.push(2u8);
            walls.push(index);
        }
    }

    let ring_size = validated_ring_layout(mesh, slices);
    let profile_segment = |triangle: &[u32; 3], ring_size: usize| {
        let mut profile = triangle.map(|vertex| vertex as usize % ring_size);
        profile.sort_unstable();
        let mut unique = profile.into_iter();
        let left = unique.next()?;
        let right = unique.find(|point| *point != left)?;
        if unique.any(|point| point != left && point != right) {
            return None;
        }
        Some(((left.min(right) as u64) << 32) | left.max(right) as u64)
    };
    // A ruled side wall is only groupable by profile segment, which needs the
    // ring layout; a planar one can be recovered from coplanarity alone.
    let coplanar = (ring_size.is_none() && !matches!(boundary, ProfileBoundary::Smooth))
        .then(|| coplanar_groups(mesh, &walls));

    let mut patches = BTreeMap::new();
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| match kind {
            0 => intern_patch(&mut patches, (0u8, 0u64)),
            1 => intern_patch(&mut patches, (1u8, 0u64)),
            _ => {
                let group = match (boundary, ring_size, &coplanar) {
                    // Every facet of a sampled curve belongs to one swept surface.
                    (ProfileBoundary::Smooth, _, _) => Some(0),
                    (_, Some(ring_size), _) => profile_segment(&mesh.tris[index], ring_size),
                    (_, None, Some(groups)) => groups.get(&index).copied(),
                    (_, None, None) => Some(0),
                };
                match group {
                    Some(group) => intern_patch(&mut patches, (2u8, group)),
                    None => UNCLASSIFIED_PATCH_ID,
                }
            }
        })
        .collect()
}

fn rotate_extrude_patch_ids(
    mesh: &Mesh,
    angle: f64,
    frags: openrscad_ir::FragmentSpec,
    boundary: ProfileBoundary,
) -> Vec<u64> {
    let sweep = angle.clamp(-360.0, 360.0);
    if sweep.abs() < 1e-12 || mesh.verts.is_empty() {
        return Vec::new();
    }
    let full = (sweep.abs() - 360.0).abs() < 1e-9;
    let max_r = mesh
        .verts
        .iter()
        .map(|point| point[0].hypot(point[1]))
        .fold(0.0_f64, f64::max);
    let full_steps = super::tessellate::fragments(max_r, frags).max(3);
    let steps = if full {
        full_steps
    } else {
        ((full_steps as f64 * sweep.abs() / 360.0).ceil() as u32).max(1)
    };
    let ring_count = if full { steps } else { steps + 1 } as usize;

    let mut union = UnionFind::new(mesh.verts.len());
    for triangle in &mesh.tris {
        union.union(triangle[0] as usize, triangle[1] as usize);
        union.union(triangle[1] as usize, triangle[2] as usize);
    }
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for vertex in 0..mesh.verts.len() {
        components
            .entry(union.find(vertex))
            .or_default()
            .push(vertex);
    }
    let mut layout = HashMap::new();
    for (component, vertices) in components {
        let base = vertices[0];
        if vertices.last().copied() != Some(base + vertices.len() - 1)
            || vertices.len() % ring_count != 0
        {
            continue;
        }
        let profile_size = vertices.len() / ring_count;
        for vertex in vertices {
            layout.insert(vertex as u32, (component, base, profile_size));
        }
    }

    let mut patches = BTreeMap::new();
    mesh.tris
        .iter()
        .map(|triangle| {
            let Some(&(component, base, profile_size)) = layout.get(&triangle[0]) else {
                return UNCLASSIFIED_PATCH_ID;
            };
            if profile_size == 0
                || triangle
                    .iter()
                    .any(|vertex| layout.get(vertex).map(|entry| entry.0) != Some(component))
            {
                return UNCLASSIFIED_PATCH_ID;
            }
            let offsets = triangle.map(|vertex| vertex as usize - base);
            let rings = offsets.map(|offset| offset / profile_size);
            if !full && rings.iter().all(|ring| *ring == 0) {
                return intern_patch(&mut patches, (component, 0u8, 0usize, 0usize));
            }
            if !full && rings.iter().all(|ring| *ring == steps as usize) {
                return intern_patch(&mut patches, (component, 1u8, 0usize, 0usize));
            }
            match boundary {
                ProfileBoundary::Smooth => {
                    return intern_patch(&mut patches, (component, 2u8, 0usize, 0usize));
                }
                ProfileBoundary::Unknown => return UNCLASSIFIED_PATCH_ID,
                ProfileBoundary::Polygonal => {}
            }
            let mut profile = offsets.map(|offset| offset % profile_size);
            profile.sort_unstable();
            let mut unique = profile.into_iter();
            let Some(left) = unique.next() else {
                return UNCLASSIFIED_PATCH_ID;
            };
            let Some(right) = unique.find(|point| *point != left) else {
                return UNCLASSIFIED_PATCH_ID;
            };
            if unique.any(|point| point != left && point != right) {
                return UNCLASSIFIED_PATCH_ID;
            }
            intern_patch(
                &mut patches,
                (component, 2u8, left.min(right), left.max(right)),
            )
        })
        .collect()
}

/// Classifies the faces of a convex hull by which operand they lie on, then
/// cuts each of those patches at its creases.
///
/// A hull's output vertices are a subset of its operands' vertices, so a face
/// whose corners all came from one operand lies on that operand's surface and
/// inherits its identity. Faces spanning operands form the ruled band between
/// them and are keyed by the set they span, which keeps an A–B band distinct
/// from an A–C one.
///
/// Operand identity alone is too coarse to derive edges from: hulling two
/// cylinders files each operand's flat caps *and* its curved wall under one
/// patch, so the 90° rim between them falls inside a patch and never gets
/// drawn, and the ruled band's four mutually perpendicular planes share one
/// patch too. The [`smooth_components`] pass therefore splits every hull patch
/// wherever adjacent faces meet at [`crate::export3d::PATCH_MERGE_DEGREES`] or
/// more; the boundary-component stage in `export3d` then rejoins whatever is
/// genuinely smooth across those cuts. A hulled coarse primitive keeps its
/// facet creases as a result — above that threshold they are real creases, and
/// splitting is what makes rims and cap outlines appear at all.
///
/// Without this every hull is a single unclassified blob, and its boundary with
/// anything it is later cut by can only be judged edge by edge — which erodes
/// the seam wherever the two surfaces approach tangency.
fn hull_patch_ids(mesh: &Mesh, children: &[Node], ctx: &mut Ctx) -> Vec<u64> {
    let key =
        |point: &[f64; 3]| point.map(|value| super::export3d::canonical_zero(value).to_bits());
    let mut owners: HashMap<[u64; 3], usize> = HashMap::new();
    for (index, child) in children.iter().enumerate() {
        let Ok(child_mesh) = render_node(child, ctx) else {
            return vec![UNCLASSIFIED_PATCH_ID; mesh.tris.len()];
        };
        for vertex in &child_mesh.verts {
            owners.entry(key(vertex)).or_insert(index);
        }
    }
    let mut patches = BTreeMap::new();
    let operands: Vec<u64> = mesh
        .tris
        .iter()
        .map(|triangle| {
            let mut spans: Vec<usize> = triangle
                .iter()
                .filter_map(|vertex| owners.get(&key(&mesh.verts[*vertex as usize])).copied())
                .collect();
            if spans.len() != 3 {
                return UNCLASSIFIED_PATCH_ID;
            }
            spans.sort_unstable();
            spans.dedup();
            intern_patch(&mut patches, spans)
        })
        .collect();
    smooth_components(mesh, &operands)
}

/// Re-interns `patches` so that each one is split into the pieces a crease of
/// at least [`crate::export3d::PATCH_MERGE_DEGREES`] separates.
///
/// Scoped to hull faces on purpose: a plain primitive's classification already
/// names its real surfaces, and splitting those would draw every tessellation
/// facet of a coarse `$fn` cylinder.
fn smooth_components(mesh: &Mesh, patches: &[u64]) -> Vec<u64> {
    let limit = super::export3d::PATCH_MERGE_DEGREES.to_radians();
    let mut union = UnionFind::new(mesh.tris.len());
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for (triangle, corners) in mesh.tris.iter().enumerate() {
        for edge in [
            [corners[0], corners[1]],
            [corners[1], corners[2]],
            [corners[2], corners[0]],
        ] {
            let key = (edge[0].min(edge[1]), edge[0].max(edge[1]));
            let Some(other) = edges.insert(key, triangle) else {
                continue;
            };
            // A zero-area triangle has no normal and reads as a right angle,
            // which cuts here rather than merging — the conservative answer.
            if patches[other] == patches[triangle]
                && patches[triangle] != UNCLASSIFIED_PATCH_ID
                && dihedral_between(mesh, other, triangle) < limit
            {
                union.union(other, triangle);
            }
        }
    }
    let mut components = BTreeMap::new();
    (0..mesh.tris.len())
        .map(|triangle| {
            if patches[triangle] == UNCLASSIFIED_PATCH_ID {
                return UNCLASSIFIED_PATCH_ID;
            }
            let component = union.find(triangle);
            intern_patch(&mut components, (patches[triangle], component))
        })
        .collect()
}

fn source_patch_ids(node: &Node, mesh: &Mesh) -> Vec<u64> {
    match node {
        Node::Cube { .. } => (0..mesh.tris.len())
            .map(|triangle| (triangle / 2) as u64)
            .collect(),
        Node::Sphere { .. } => vec![0; mesh.tris.len()],
        Node::Cylinder { .. } => {
            let Some((min, max)) = mesh.bbox() else {
                return Vec::new();
            };
            let tolerance = (max[2] - min[2]).abs() * 1e-12 + f64::EPSILON;
            mesh.tris
                .iter()
                .map(|triangle| {
                    let z = triangle.map(|vertex| mesh.verts[vertex as usize][2]);
                    if z.iter().all(|value| (*value - min[2]).abs() <= tolerance) {
                        1
                    } else if z.iter().all(|value| (*value - max[2]).abs() <= tolerance) {
                        2
                    } else {
                        0
                    }
                })
                .collect()
        }
        Node::Polyhedron { faces, .. } => mesh
            .tris
            .iter()
            .map(|triangle| {
                faces
                    .iter()
                    .position(|face| triangle.iter().all(|vertex| face.contains(vertex)))
                    .map_or(UNCLASSIFIED_PATCH_ID, |face| face as u64)
            })
            .collect(),
        Node::LinearExtrude { slices, child, .. } => {
            linear_extrude_patch_ids(mesh, *slices, profile_boundary(child))
        }
        Node::RotateExtrude {
            angle,
            frags,
            child,
        } => rotate_extrude_patch_ids(mesh, *angle, *frags, profile_boundary(child)),
        Node::Import { .. } => vec![UNCLASSIFIED_PATCH_ID; mesh.tris.len()],
        _ => vec![UNCLASSIFIED_PATCH_ID; mesh.tris.len()],
    }
}

fn initial_relation(
    mesh: Mesh,
    surface: SurfaceAttributionId,
    source_face_ids: Vec<u64>,
) -> RelationMesh {
    let triangles = mesh.tris.len();
    RelationMesh {
        attributed: AttributedMesh {
            mesh,
            surface_ids: vec![surface; triangles],
            source_face_ids,
        },
        backside: vec![false; triangles],
    }
}

fn is_boundary_node(node: &Node) -> bool {
    matches!(
        node,
        Node::Cube { .. }
            | Node::Sphere { .. }
            | Node::Cylinder { .. }
            | Node::Polyhedron { .. }
            | Node::Import { .. }
    )
}

fn render_children(
    children: &[Node],
    active: &ActiveSurface,
    ctx: &mut DetailedCtx,
) -> Result<Vec<RelationMesh>, GeomError> {
    let mut rendered = Vec::new();
    for child in children {
        rendered.extend(render_relation(child, active, ctx)?);
    }
    rendered.retain(|mesh| !mesh.attributed.mesh.is_empty());
    Ok(rendered)
}

fn append_relation_mesh(target: &mut RelationMesh, source: RelationMesh) {
    super::append_mesh(&mut target.attributed.mesh, &source.attributed.mesh);
    target
        .attributed
        .surface_ids
        .extend(source.attributed.surface_ids);
    target
        .attributed
        .source_face_ids
        .extend(source.attributed.source_face_ids);
    target.backside.extend(source.backside);
}

fn concatenate_scene(meshes: Vec<RelationMesh>) -> RelationMesh {
    let mut result = RelationMesh::empty();
    for mesh in meshes {
        append_relation_mesh(&mut result, mesh);
    }
    result
}

fn union_scene(
    meshes: Vec<RelationMesh>,
    ctx: &mut DetailedCtx,
) -> Result<RelationMesh, GeomError> {
    let fallback = meshes.clone();
    let result = record_boolean(|| ctx.relation.union(meshes, &ctx.run_keys));
    match result {
        Ok(mesh) => Ok(mesh),
        Err(error) => {
            ctx.ordinary.errors.push(format!(
                "union: {error} — showing un-combined geometry (the boolean was skipped)"
            ));
            Ok(concatenate_scene(fallback))
        }
    }
}

fn render_relation(
    node: &Node,
    active: &ActiveSurface,
    ctx: &mut DetailedCtx,
) -> Result<Vec<RelationMesh>, GeomError> {
    match node {
        Node::Empty => Ok(Vec::new()),
        Node::Background(child) => {
            if !ctx.include_backgrounds {
                return Ok(Vec::new());
            }
            let mut active = active.clone();
            active.mode = DisplayMode::Background;
            render_relation(child, &active, ctx)
        }
        Node::Color { rgba, child } => {
            let mut active = active.clone();
            active.rgba = *rgba;
            active.color_explicit = true;
            render_relation(child, &active, ctx)
        }
        Node::Highlight(child) => {
            if !ctx.include_backgrounds {
                return render_relation(child, active, ctx);
            }
            let mut active = active.clone();
            active.mode = DisplayMode::Highlight;
            render_relation(child, &active, ctx)
        }
        Node::Provenance { frame, child } => {
            let mut active = active.clone();
            active.provenance.push(frame.clone());
            render_relation(child, &active, ctx)
        }
        Node::Import { data, format } if format == "3mf" => {
            let Some((mesh, colors)) = Mesh::from_3mf_attributed(data) else {
                return Ok(Vec::new());
            };
            let source_face_ids = vec![UNCLASSIFIED_PATCH_ID; mesh.tris.len()];
            let surface_ids = colors
                .into_iter()
                .map(|color| {
                    let mut surface = active.clone();
                    if !surface.color_explicit {
                        if let Some(color) = color {
                            surface.rgba = color;
                        }
                    }
                    ctx.active_id(
                        &surface,
                        ContributionKind::Boundary,
                        AttributionStatus::Exact,
                    )
                })
                .collect();
            let triangles = mesh.tris.len();
            Ok(vec![RelationMesh {
                attributed: AttributedMesh {
                    mesh,
                    surface_ids,
                    source_face_ids,
                },
                backside: vec![false; triangles],
            }])
        }
        Node::Group(children) => render_children(children, active, ctx),
        Node::Union(children) => Ok(vec![union_scene(
            render_children(children, active, ctx)?,
            ctx,
        )?]),
        Node::Translate { v, child } => {
            let mut meshes = render_relation(child, active, ctx)?;
            for mesh in &mut meshes {
                translate(&mut mesh.attributed.mesh, *v);
            }
            Ok(meshes)
        }
        Node::Rotate { deg, child } => {
            let mut meshes = render_relation(child, active, ctx)?;
            for mesh in &mut meshes {
                rotate(&mut mesh.attributed.mesh, *deg);
            }
            Ok(meshes)
        }
        Node::Scale { v, child } => {
            let mut meshes = render_relation(child, active, ctx)?;
            for mesh in &mut meshes {
                scale(&mut mesh.attributed.mesh, *v);
            }
            Ok(meshes)
        }
        Node::Mirror { v, child } => {
            let mut meshes = render_relation(child, active, ctx)?;
            for mesh in &mut meshes {
                mirror(&mut mesh.attributed.mesh, *v);
            }
            Ok(meshes)
        }
        Node::MultMatrix { m, child } => {
            let mut meshes = render_relation(child, active, ctx)?;
            for mesh in &mut meshes {
                mult_matrix(&mut mesh.attributed.mesh, m);
            }
            Ok(meshes)
        }
        Node::Difference(children) if !is_2d(node) => {
            let Some((base_node, tool_nodes)) = children.split_first() else {
                return Ok(Vec::new());
            };
            let base_relations = render_relation(base_node, active, ctx)?;
            let base = union_scene(base_relations, ctx)?;
            let base_ids: HashSet<_> = base.attributed.surface_ids.iter().copied().collect();
            let tools = render_children(tool_nodes, active, ctx)?;
            let mut contributor_ids = base_ids.clone();
            contributor_ids.extend(
                tools
                    .iter()
                    .flat_map(|tool| tool.attributed.surface_ids.iter().copied()),
            );
            let fallback = base.clone();
            let result = record_boolean(|| ctx.relation.difference(base, tools, &ctx.run_keys));
            let mut result = match result {
                Ok(result) => result,
                Err(error) => {
                    ctx.ordinary.errors.push(format!(
                        "difference: {error} — showing un-combined geometry (the boolean was skipped)"
                    ));
                    fallback
                }
            };
            if !result.attributed.mesh.is_empty() {
                let appearance = *base_ids.iter().min().expect("non-empty difference base");
                let owner = ctx.generated_id(
                    appearance,
                    &contributor_ids,
                    active,
                    ContributionKind::DifferenceTool,
                );
                for (surface, backside) in result
                    .attributed
                    .surface_ids
                    .iter_mut()
                    .zip(result.backside.iter_mut())
                {
                    if *backside || !base_ids.contains(surface) {
                        *surface = owner;
                    }
                    *backside = false;
                }
            }
            Ok(vec![result])
        }
        Node::Intersection(children) if !is_2d(node) => {
            let Some((first_node, remaining)) = children.split_first() else {
                return Ok(Vec::new());
            };
            let first_relations = render_relation(first_node, active, ctx)?;
            let first = union_scene(first_relations, ctx)?;
            let first_ids: HashSet<_> = first.attributed.surface_ids.iter().copied().collect();
            let mut operands = vec![first];
            for operand in remaining {
                let relations = render_relation(operand, active, ctx)?;
                operands.push(union_scene(relations, ctx)?);
            }
            let contributor_ids: HashSet<_> = operands
                .iter()
                .flat_map(|operand| operand.attributed.surface_ids.iter().copied())
                .collect();
            let fallback = operands.clone();
            let result = record_boolean(|| ctx.relation.intersection(operands, &ctx.run_keys));
            let mut result = match result {
                Ok(result) => result,
                Err(error) => {
                    ctx.ordinary.errors.push(format!(
                        "intersection: {error} — showing un-combined geometry (the boolean was skipped)"
                    ));
                    concatenate_scene(fallback)
                }
            };
            if !result.attributed.mesh.is_empty() {
                let appearance = *first_ids
                    .iter()
                    .min()
                    .expect("non-empty intersection first operand");
                for surface in &mut result.attributed.surface_ids {
                    let appearance = if first_ids.contains(surface) {
                        *surface
                    } else {
                        appearance
                    };
                    *surface = ctx.generated_id(
                        appearance,
                        &contributor_ids,
                        active,
                        ContributionKind::IntersectionOperand,
                    );
                }
                result.backside.fill(false);
            }
            Ok(vec![result])
        }
        _ => {
            let mesh = render_node(node, ctx.ordinary)?;
            if mesh.is_empty() {
                return Ok(Vec::new());
            }
            let contribution = if is_boundary_node(node) {
                ContributionKind::Boundary
            } else {
                ContributionKind::GeneratedOpaqueOperation
            };
            let surface = ctx.active_id(active, contribution, AttributionStatus::Exact);
            let local_patches = match node {
                Node::Hull(children) => hull_patch_ids(&mesh, children, ctx.ordinary),
                _ => source_patch_ids(node, &mesh),
            };
            let source_face_ids = ctx.global_patch_ids(local_patches);
            Ok(vec![initial_relation(mesh, surface, source_face_ids)])
        }
    }
}

fn render_structured_with(
    node: &Node,
    kernel: &dyn Kernel,
    relation: &dyn RelationKernel,
    cache: &mut GeomCache,
    include_backgrounds: bool,
) -> Result<(StructuredMesh, super::RenderDiagnostics), GeomError> {
    let mut hashes = HashMap::new();
    hash_all(node, &mut hashes);
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut ordinary = Ctx {
        kernel,
        cache,
        mode: RenderMode::Exact,
        hashes: &hashes,
        warnings: &mut warnings,
        errors: &mut errors,
    };
    let mut detailed = DetailedCtx {
        ordinary: &mut ordinary,
        relation,
        surfaces: SurfaceTable::default(),
        include_backgrounds,
        next_patch_id: 0,
        run_keys: RunKeys::default(),
    };
    #[cfg(feature = "benchmark-profile")]
    let render_started = Instant::now();
    let result = concatenate_scene(render_relation(
        node,
        &ActiveSurface::default(),
        &mut detailed,
    )?);
    let aggregate = render_node(node, detailed.ordinary)?;
    #[cfg(feature = "benchmark-profile")]
    BENCHMARK_PROFILE.with(|profile| {
        profile.borrow_mut().attributed_render_ms =
            render_started.elapsed().as_secs_f64() * 1_000.0;
    });
    result.validate()?;
    #[cfg(feature = "benchmark-profile")]
    let partition_started = Instant::now();
    let partitioned = partition(
        result.attributed,
        aggregate,
        include_backgrounds,
        detailed.surfaces.values,
    )?;
    #[cfg(feature = "benchmark-profile")]
    BENCHMARK_PROFILE.with(|profile| {
        profile.borrow_mut().partition_ms = partition_started.elapsed().as_secs_f64() * 1_000.0;
    });
    warnings.sort();
    warnings.dedup();
    errors.sort();
    errors.dedup();
    Ok((partitioned, super::RenderDiagnostics { warnings, errors }))
}

#[cfg(test)]
pub(crate) fn render_structured_rust_cached(
    node: &Node,
    kernel: &dyn Kernel,
    cache: &mut GeomCache,
) -> Result<StructuredMesh, GeomError> {
    render_structured_with(node, kernel, &RustRelationKernel, cache, false).map(|result| result.0)
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
pub(crate) fn render_structured_native_cached(
    node: &Node,
    kernel: &dyn Kernel,
    cache: &mut GeomCache,
) -> Result<StructuredMesh, GeomError> {
    render_structured_with(node, kernel, &NativeRelationKernel, cache, false).map(|result| result.0)
}

pub fn render_structured_cached_diag(
    node: &Node,
    kernel: &dyn Kernel,
    cache: &mut GeomCache,
    include_backgrounds: bool,
) -> Result<(StructuredMesh, super::RenderDiagnostics), GeomError> {
    #[cfg(target_arch = "wasm32")]
    let relation = &RustRelationKernel as &dyn RelationKernel;
    #[cfg(not(target_arch = "wasm32"))]
    let relation = &NativeRelationKernel as &dyn RelationKernel;
    render_structured_with(node, kernel, relation, cache, include_backgrounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrscad_ir::{SourceId, SourceSpan};

    fn color(rgba: [f32; 4], child: Node) -> Node {
        Node::Color {
            rgba,
            child: Box::new(child),
        }
    }

    fn cube_at(z: f64) -> Node {
        Node::Translate {
            v: [0.0, 0.0, z],
            child: Box::new(Node::Cube {
                size: [1.0, 1.0, 1.0],
                center: false,
            }),
        }
    }

    #[test]
    fn relation_output_merge_metadata_restores_indexed_topology() {
        let keys = RunKeys::default();
        let run = keys.intern(SurfaceAttributionId(0), 7).unwrap();
        let relation = relation_from_parts(
            RelationParts {
                num_prop: 3,
                vertex_properties: &[
                    0.0, 0.0, 0.0, // 0
                    1.0, 0.0, 0.0, // 1
                    0.0, 1.0, 0.0, // 2
                    0.0, 0.0, 0.0, // 3 aliases 0
                    0.0, 0.0, 1.0, // 4
                ],
                triangle_vertices: &[0, 1, 2, 3, 2, 4],
                run_index: &[0, 6],
                run_original_id: &[run],
                merge_from_vert: &[3],
                merge_to_vert: &[0],
                run_flags: &[],
            },
            &keys,
        )
        .unwrap();
        let triangles = vec![0, 1];

        assert_eq!(
            components_for_triangles(&relation.attributed.mesh, &triangles).len(),
            1
        );
        assert_eq!(relation.attributed.mesh.tris[1][0], 0);
        assert_eq!(relation.attributed.surface_ids[0], SurfaceAttributionId(0));
        assert_eq!(relation.attributed.source_face_ids, vec![7, 7]);
    }

    fn authored(name: &str, call_start: u32, child: Node) -> Node {
        Node::Provenance {
            frame: ProvenanceFrame {
                call_site: SourceSpan {
                    source_id: SourceId(0),
                    start: call_start,
                    end: call_start + name.len() as u32,
                },
                definition_site: None,
                module_name: Some(name.to_string()),
            },
            child: Box::new(child),
        }
    }

    fn referenced_surfaces(
        structured: &StructuredMesh,
        contribution: ContributionKind,
    ) -> Vec<&SurfaceAttribution> {
        let ids: std::collections::BTreeSet<_> =
            structured.exact.surface_ids.iter().copied().collect();
        ids.into_iter()
            .map(|id| &structured.attributions[id.0 as usize])
            .filter(|surface| surface.contribution == contribution)
            .collect()
    }

    #[test]
    fn cube_tessellation_has_six_source_patches() {
        let mesh = super::super::cube([1.0, 1.0, 1.0], false);
        let ids = source_patch_ids(
            &Node::Cube {
                size: [1.0, 1.0, 1.0],
                center: false,
            },
            &mesh,
        );
        assert_eq!(ids.len(), 12);
        assert_eq!(ids.into_iter().collect::<HashSet<_>>().len(), 6);
    }

    #[test]
    fn same_color_separated_cubes_are_two_min_z_ordered_parts() {
        let node = color(
            [1.0, 0.0, 0.0, 1.0],
            Node::Union(vec![cube_at(5.0), cube_at(-5.0)]),
        );
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();

        assert_eq!(structured.solid_components.len(), 2);
        assert_eq!(structured.solid_components[0].bounds.min[2], -5.0);
        assert_eq!(structured.solid_components[1].bounds.min[2], 5.0);
    }

    #[test]
    fn connected_multicolor_boundary_has_color_parts_but_one_solid() {
        let node = Node::Union(vec![
            color(
                [1.0, 0.0, 0.0, 1.0],
                Node::Cube {
                    size: [2.0, 2.0, 2.0],
                    center: false,
                },
            ),
            color(
                [0.0, 0.0, 1.0, 1.0],
                Node::Translate {
                    v: [1.0, 0.0, 0.0],
                    child: Box::new(Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    }),
                },
            ),
        ]);
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();

        assert_eq!(structured.solid_components.len(), 1);
        assert_eq!(
            structured
                .exact
                .surface_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn overlapping_same_color_inputs_are_one_exact_component() {
        let node = color(
            [1.0, 0.0, 0.0, 1.0],
            Node::Union(vec![
                Node::Cube {
                    size: [2.0, 2.0, 2.0],
                    center: false,
                },
                Node::Translate {
                    v: [1.0, 0.0, 0.0],
                    child: Box::new(Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    }),
                },
            ]),
        );
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();

        assert_eq!(structured.solid_components.len(), 1);
        assert!((structured.exact.mesh.volume() - 12.0).abs() < 1e-6);
    }

    #[test]
    fn partition_uses_indexed_connectivity_without_spatial_welding() {
        let mut first = super::super::cube([1.0, 1.0, 1.0], false);
        let mut second = super::super::cube([1.0, 1.0, 1.0], false);
        translate(&mut second, [1.0, 1.0, 1.0]);
        super::super::append_mesh(&mut first, &second);
        let triangle_count = first.tris.len();
        let exact = AttributedMesh {
            mesh: first,
            surface_ids: vec![SurfaceAttributionId(0); triangle_count],
            source_face_ids: (0..triangle_count as u64).collect(),
        };
        let aggregate = exact.mesh.clone();
        let structured = partition(
            exact,
            aggregate,
            false,
            vec![SurfaceAttribution {
                rgba: DEFAULT_COLOR,
                mode: DisplayMode::Solid,
                provenance: Vec::new(),
                contributors: Vec::new(),
                contribution: ContributionKind::Boundary,
                status: AttributionStatus::Exact,
            }],
        )
        .unwrap();

        assert_eq!(structured.solid_components.len(), 2);
    }

    #[test]
    fn partition_rejects_invalid_relation_lengths() {
        let exact = AttributedMesh {
            mesh: super::super::cube([1.0, 1.0, 1.0], false),
            surface_ids: vec![SurfaceAttributionId(0)],
            source_face_ids: vec![0],
        };
        let aggregate = exact.mesh.clone();
        let error = partition(exact, aggregate, false, Vec::new()).unwrap_err();
        assert!(matches!(error, GeomError::Invariant(_)));
    }

    #[test]
    fn structured_result_is_deterministic_and_retains_provenance() {
        let frame = ProvenanceFrame {
            call_site: SourceSpan {
                source_id: SourceId(0),
                start: 10,
                end: 20,
            },
            definition_site: None,
            module_name: Some("part".to_string()),
        };
        let node = Node::Provenance {
            frame: frame.clone(),
            child: Box::new(Node::Union(vec![cube_at(2.0), cube_at(-2.0)])),
        };
        let render = || {
            render_structured_rust_cached(
                &node,
                &super::super::RustManifoldKernel::new(),
                &mut GeomCache::new(),
            )
            .unwrap()
        };

        let first = render();
        assert_eq!(first, render());
        assert!(first.attributions.iter().all(|surface| {
            surface.provenance.is_empty() || surface.provenance == vec![frame.clone()]
        }));
    }

    #[test]
    fn difference_generated_faces_use_enclosing_owner_with_all_contributor_evidence() {
        let base = Node::Union(vec![
            authored(
                "left",
                20,
                Node::Cube {
                    size: [2.0, 2.0, 2.0],
                    center: false,
                },
            ),
            authored(
                "right",
                30,
                Node::Translate {
                    v: [2.0, 0.0, 0.0],
                    child: Box::new(Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    }),
                },
            ),
        ]);
        let node = authored(
            "assembly",
            10,
            Node::Difference(vec![
                base,
                Node::Translate {
                    v: [1.5, -1.0, 0.5],
                    child: Box::new(Node::Cube {
                        size: [1.0, 4.0, 1.0],
                        center: false,
                    }),
                },
            ]),
        );
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let generated = referenced_surfaces(&structured, ContributionKind::DifferenceTool);

        assert_eq!(generated.len(), 1);
        assert_eq!(
            generated[0]
                .provenance
                .iter()
                .map(|frame| frame.module_name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["assembly"]
        );
        assert_eq!(generated[0].contributors.len(), 3);
        assert_eq!(generated[0].status, AttributionStatus::Ambiguous);
    }

    #[test]
    fn generated_faces_with_one_authored_owner_are_exact_even_across_colors() {
        let node = authored(
            "painted_part",
            10,
            Node::Difference(vec![
                Node::Union(vec![
                    color(
                        [1.0, 0.0, 0.0, 1.0],
                        Node::Cube {
                            size: [2.0, 2.0, 2.0],
                            center: false,
                        },
                    ),
                    color(
                        [0.0, 0.0, 1.0, 1.0],
                        Node::Translate {
                            v: [2.0, 0.0, 0.0],
                            child: Box::new(Node::Cube {
                                size: [2.0, 2.0, 2.0],
                                center: false,
                            }),
                        },
                    ),
                ]),
                Node::Translate {
                    v: [1.5, -1.0, 0.5],
                    child: Box::new(Node::Cube {
                        size: [1.0, 4.0, 1.0],
                        center: false,
                    }),
                },
            ]),
        );
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let generated = referenced_surfaces(&structured, ContributionKind::DifferenceTool);

        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].contributors.len(), 1);
        assert_eq!(generated[0].status, AttributionStatus::Exact);
        assert_eq!(
            generated[0].provenance[0].module_name.as_deref(),
            Some("painted_part")
        );
    }

    #[test]
    fn generated_faces_without_common_authored_owner_remain_anonymous() {
        let node = Node::Difference(vec![
            Node::Union(vec![
                authored(
                    "left",
                    10,
                    Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    },
                ),
                authored(
                    "right",
                    20,
                    Node::Translate {
                        v: [2.0, 0.0, 0.0],
                        child: Box::new(Node::Cube {
                            size: [2.0, 2.0, 2.0],
                            center: false,
                        }),
                    },
                ),
            ]),
            Node::Translate {
                v: [1.5, -1.0, 0.5],
                child: Box::new(Node::Cube {
                    size: [1.0, 4.0, 1.0],
                    center: false,
                }),
            },
        ]);
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let generated = referenced_surfaces(&structured, ContributionKind::DifferenceTool);

        assert_eq!(generated.len(), 1);
        assert!(generated[0].provenance.is_empty());
        assert_eq!(generated[0].contributors.len(), 2);
        assert_eq!(generated[0].status, AttributionStatus::Ambiguous);
    }

    #[test]
    fn intersection_generated_faces_use_first_operands_common_owner() {
        let first = authored(
            "assembly",
            10,
            Node::Union(vec![
                authored(
                    "left",
                    20,
                    Node::Cube {
                        size: [2.0, 2.0, 2.0],
                        center: false,
                    },
                ),
                authored(
                    "right",
                    30,
                    Node::Translate {
                        v: [2.0, 0.0, 0.0],
                        child: Box::new(Node::Cube {
                            size: [2.0, 2.0, 2.0],
                            center: false,
                        }),
                    },
                ),
            ]),
        );
        let node = Node::Intersection(vec![
            first,
            Node::Translate {
                v: [1.0, -1.0, -1.0],
                child: Box::new(Node::Cube {
                    size: [2.0, 4.0, 4.0],
                    center: false,
                }),
            },
        ]);
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let generated = referenced_surfaces(&structured, ContributionKind::IntersectionOperand);

        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].provenance.len(), 1);
        assert_eq!(
            generated[0].provenance[0].module_name.as_deref(),
            Some("assembly")
        );
        assert_eq!(generated[0].contributors.len(), 2);
        assert_eq!(generated[0].status, AttributionStatus::Ambiguous);
    }

    #[test]
    fn opaque_operations_keep_the_active_authored_owner() {
        let node = authored(
            "rounded_part",
            10,
            Node::Hull(vec![
                Node::Cube {
                    size: [1.0, 1.0, 1.0],
                    center: false,
                },
                Node::Translate {
                    v: [2.0, 0.0, 0.0],
                    child: Box::new(Node::Cube {
                        size: [1.0, 1.0, 1.0],
                        center: false,
                    }),
                },
            ]),
        );
        let structured = render_structured_rust_cached(
            &node,
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let generated =
            referenced_surfaces(&structured, ContributionKind::GeneratedOpaqueOperation);

        assert_eq!(generated.len(), 1);
        assert_eq!(
            generated[0].provenance[0].module_name.as_deref(),
            Some("rounded_part")
        );
        assert!(generated[0].contributors.is_empty());
        assert_eq!(generated[0].status, AttributionStatus::Exact);
    }

    /// Keying a hull face by operand alone files a cylinder's two flat caps and
    /// its curved wall under one patch, and the whole ruled band under another,
    /// so the 90° rims land inside a patch and never reach the edge derivation.
    /// A capsule has to come out as ten surfaces: wall, top cap and bottom cap
    /// per operand, plus the band's top strip, bottom strip and two tangent
    /// side planes.
    #[test]
    fn hulled_capsule_splits_into_its_smooth_surfaces() {
        use std::collections::BTreeSet;

        let end = |x: f64| Node::Translate {
            v: [x, 0.0, 0.0],
            child: Box::new(Node::Cylinder {
                h: 4.0,
                r1: 2.5,
                r2: 2.5,
                center: false,
                frags: openrscad_ir::FragmentSpec {
                    fn_: 32.0,
                    ..Default::default()
                },
            }),
        };
        let structured = render_structured_rust_cached(
            &Node::Hull(vec![end(-8.0), end(8.0)]),
            &super::super::RustManifoldKernel::new(),
            &mut GeomCache::new(),
        )
        .unwrap();
        let patches: BTreeSet<_> = structured.exact.source_face_ids.iter().copied().collect();

        assert!(!patches.contains(&UNCLASSIFIED_PATCH_ID));
        assert_eq!(patches.len(), 10);
    }

    /// Patch identity has to survive whichever Manifold implementation the build
    /// links: `manifold-rust` on Wasm, `manifold-csg` natively. Before the run
    /// channel carried `(surface, patch)` the two disagreed, and a model exported
    /// from Tau drew edges the native CLI never showed. The two still tessellate
    /// booleans at different densities, so the triangle and segment totals differ;
    /// what must not differ is the classification.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn both_relation_kernels_classify_a_boolean_into_the_same_patches() {
        use std::collections::BTreeSet;

        let frags = openrscad_ir::FragmentSpec {
            fn_: 32.0,
            ..Default::default()
        };
        let cube = Node::Cube {
            size: [20.0, 20.0, 20.0],
            center: true,
        };
        let sphere = Node::Sphere { r: 12.0, frags };

        // Hull is deliberately absent here: the two implementations triangulate
        // a hull differently, so the census of its patches differs even though
        // the surfaces do not. `both_relation_kernels_draw_the_same_hull_feature_edges`
        // covers hull at the level that is actually observable — the segments.
        for node in [
            Node::Difference(vec![cube.clone(), sphere.clone()]),
            Node::Intersection(vec![cube, Node::Sphere { r: 13.0, frags }]),
        ] {
            let patches = |mesh: &StructuredMesh| {
                mesh.exact
                    .source_face_ids
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
            };
            let rust = render_structured_rust_cached(
                &node,
                &super::super::RustManifoldKernel::new(),
                &mut GeomCache::new(),
            )
            .unwrap();
            let native = render_structured_native_cached(
                &node,
                &super::super::ManifoldKernel::new(),
                &mut GeomCache::new(),
            )
            .unwrap();

            assert_eq!(patches(&rust).len(), patches(&native).len());
            for mesh in [&rust, &native] {
                assert!(!patches(mesh).contains(&UNCLASSIFIED_PATCH_ID));
                assert_eq!(patches(mesh).len(), 7);
            }
        }
    }
}
