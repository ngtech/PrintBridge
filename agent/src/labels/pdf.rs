use super::config::Profile;
use anyhow::{bail, ensure, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashSet;

pub const PDF_TOLERANCE_MM: f64 = 0.10;
const MAX_PAGES: usize = 100;
const MAX_TREE_NODES: usize = 2048;

pub fn mm_to_points(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

fn resolve<'doc>(document: &'doc Document, mut value: &'doc Object) -> Result<&'doc Object> {
    for _ in 0..32 {
        if let Object::Reference(id) = value {
            value = document.objects.get(id).context("Missing PDF object")?;
        } else {
            return Ok(value);
        }
    }
    bail!("Invalid PDF reference chain")
}

fn number(value: &Object) -> Result<f64> {
    match value {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) if value.is_finite() => Ok(f64::from(*value)),
        _ => bail!("Invalid PDF page box coordinate"),
    }
}

fn page_box(document: &Document, value: &Object) -> Result<[f64; 4]> {
    let coordinates = resolve(document, value)?.as_array()?;
    ensure!(coordinates.len() == 4, "PDF page box must have four coordinates");
    let mut bounds = [0.0; 4];
    for (bound, coordinate) in bounds.iter_mut().zip(coordinates) {
        *bound = number(resolve(document, coordinate)?)?;
    }
    Ok(bounds)
}

#[derive(Clone, Copy, Default)]
struct Inherited {
    media: Option<[f64; 4]>,
    crop: Option<[f64; 4]>,
    rotation: i64,
}

fn inherited(document: &Document, node: &Dictionary, mut parent: Inherited) -> Result<Inherited> {
    if let Ok(value) = node.get(b"MediaBox") {
        parent.media = Some(page_box(document, value)?);
    }
    if let Ok(value) = node.get(b"CropBox") {
        parent.crop = Some(page_box(document, value)?);
    }
    if let Ok(value) = node.get(b"Rotate") {
        parent.rotation = resolve(document, value)?.as_i64()?;
    }
    Ok(parent)
}

fn dimensions(bytes: &[u8]) -> Result<Vec<(f64, f64)>> {
    ensure!(bytes.starts_with(b"%PDF-"), "Document is not a PDF");
    let document = Document::load_mem(bytes).context("Cannot read PDF")?;
    ensure!(document.trailer.get(b"Encrypt").is_err(), "Encrypted PDFs are not supported");
    let root = resolve(&document, document.trailer.get(b"Root")?)?.as_dict()?;
    let page_root = root.get(b"Pages")?.as_reference()?;
    let mut stack = vec![(page_root, None::<ObjectId>, Inherited::default(), 0)];
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    while let Some((id, parent_id, geometry, depth)) = stack.pop() {
        ensure!(depth <= 32 && visited.len() < MAX_TREE_NODES && visited.insert(id),
            "PDF page tree is cyclic or exceeds limits");
        let node = document.objects.get(&id).context("Missing PDF page tree node")?.as_dict()?;
        if let Some(parent) = parent_id {
            ensure!(node.get(b"Parent")?.as_reference()? == parent, "Inconsistent PDF page tree");
        }
        let geometry = inherited(&document, node, geometry)?;
        match resolve(&document, node.get(b"Type")?)?.as_name()? {
            b"Pages" => {
                let children = resolve(&document, node.get(b"Kids")?)?.as_array()?;
                ensure!(stack.len() + children.len() <= MAX_TREE_NODES, "Too many PDF page tree nodes");
                for child in children.iter().rev() {
                    stack.push((child.as_reference()?, Some(id), geometry, depth + 1));
                }
            }
            b"Page" => {
                let page = result.len() + 1;
                ensure!(page <= MAX_PAGES, "PDF exceeds {MAX_PAGES} pages");
                ensure!(geometry.rotation.rem_euclid(360) == 0, "Page {page}: PDF Rotate must be zero");
                if let Ok(unit) = node.get(b"UserUnit") {
                    ensure!(number(resolve(&document, unit)?)? == 1.0, "Page {page}: nonstandard PDF UserUnit");
                }
                let bounds = geometry.media.context("PDF page has no MediaBox")?;
                ensure!(bounds[0] == 0.0 && bounds[1] == 0.0 && bounds[2] > 0.0 && bounds[3] > 0.0,
                    "Page {page}: MediaBox must start at (0,0) with positive dimensions");
                ensure!(geometry.crop.map_or(true, |crop| crop == bounds),
                    "Page {page}: CropBox differs from MediaBox");
                result.push((bounds[2], bounds[3]));
            }
            _ => bail!("Invalid PDF page tree node type"),
        }
    }
    ensure!(!result.is_empty(), "PDF contains no pages");
    let root_count = document.objects.get(&page_root).context("Missing PDF page root")?
        .as_dict()?.get(b"Count")?;
    ensure!(resolve(&document, root_count)?.as_i64()? == result.len() as i64,
        "Inconsistent PDF page count");
    Ok(result)
}

pub fn validate(bytes: &[u8], profile: &Profile, copies: u16) -> Result<()> {
    let width = mm_to_points(profile.width_mm);
    let height = mm_to_points(profile.height_mm);
    let tolerance = mm_to_points(PDF_TOLERANCE_MM);
    let pages = dimensions(bytes)?;
    ensure!(pages.len() * usize::from(copies) <= 500, "Job exceeds 500 printed pages");
    for (index, (actual_width, actual_height)) in pages.into_iter().enumerate() {
        ensure!((actual_width - width).abs() <= tolerance && (actual_height - height).abs() <= tolerance,
            "Page {} is {:.2} × {:.2} mm; expected {:.2} × {:.2} mm (±{PDF_TOLERANCE_MM:.2} mm)",
            index + 1, actual_width * 25.4 / 72.0, actual_height * 25.4 / 72.0,
            profile.width_mm, profile.height_mm);
    }
    Ok(())
}
