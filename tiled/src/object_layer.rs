use crate::*;

#[derive(Debug, Clone)]
pub enum ObjGroupType {
    ObjectGroup,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub struct ObjectId {
    id: u32,
    from_obj_layer: bool,
}

impl ObjectId {
    pub fn new(id: u32, from_obj_layer: bool) -> Self {
        ObjectId { id, from_obj_layer }
    }

    pub fn tainted_new(id: u32) -> Self {
        ObjectId {
            id,
            from_obj_layer: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Halign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone)]
pub enum Valign {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct Text {
    pub wrapping: bool,
    pub text: String,
    pub fontfamily: String,
    pub pixelsize: u32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub kerning: bool,
    pub halign: Halign,
    pub valign: Valign,
}

#[derive(Debug, Clone, Copy)]
pub struct ObjectRef(pub usize);

#[derive(Debug, PartialEq, Clone)]
pub enum ObjectShape {
    Rect,
    Ellipse,
    Polyline { points: Vec<(f32, f32)> },
    Polygon { points: Vec<(f32, f32)> },
    Point,
}

impl ObjectShape {
    pub fn from_string(s: &str) -> Result<Self, Error> {
        match s {
            "rectangle" => Ok(ObjectShape::Rect),
            "ellipse" => Ok(ObjectShape::Ellipse),
            "point" => Ok(ObjectShape::Point),
            s if s == "polygon" || s == "polyline" => {
                Err(anyhow!("{} objects aren't supported yet, ping Maxim", s))
            }
            e => Err(anyhow!("Got an unsupported shape type: {}", e)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Object {
    pub id: ObjectId,
    pub name: String,
    pub obj_type: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub tile_id: Option<TileId>,
    pub visible: bool,
    pub properties: Properties,
    pub shape: Option<ObjectShape>,
    pub text: Option<Text>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ObjectLayerId {
    // global layer id and local layer id
    // global layer id is set by tiled, local layer id is generated sequentially in the order
    // that the layers are parsed
    pub glid: u32,
    pub llid: u32,
}

#[derive(Debug, Clone)]
pub enum DrawOrder {
    TopDown,
    Index,
}

#[derive(Debug, Clone)]
pub struct ObjectGroup {
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub draworder: DrawOrder,
    pub object_refs: Vec<ObjectRef>,
    // TODO: maybe change this to Vec<(String, ObjectId)>?
    pub object_name_map: HashMap<String, Vec<ObjectId>>,
    pub color: Color,
    pub id: ObjectLayerId,
    pub obj_group_type: ObjGroupType,
    /**
     * Layer index is not preset for tile collision boxes
     */
    pub layer_index: Option<u32>,
    pub properties: Properties,
    pub tintcolor: Option<Color>,
    pub off_x: u32,
    pub off_y: u32,
}

impl ObjectGroup {
    pub fn get_obj_refs(&self) -> impl Iterator<Item = &ObjectRef> + '_ {
        self.object_refs.iter()
    }
}

pub type ObjectLayer = ObjectGroup;
