use generational_arena::{Arena, Index};

use std::ops::Add;

#[derive(Debug, Clone)]
pub struct Point {
    x: f32,
    y: f32,
}
impl Point {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

#[derive(Debug)]
pub struct Square {
    position: Point,
    pub parent: Option<Index>,
    pub child: Option<Index>,
}

impl Square {
    pub fn new() -> Self {
        Self {
            position: Point::new(),
            parent: None,
            child: None,
        }
    }
    pub fn move_by(&mut self, x: f32, y: f32) {
        self.position.x += x;
        self.position.y += y;
    }
    pub fn set_child(&mut self, idx: Index) {
        self.child = Some(idx);
    }
    pub fn set_parent(&mut self, idx: Index) {
        self.parent = Some(idx);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SquareIndex(Index);

impl SquareIndex {
    pub fn new(idx: Index) -> Self {
        Self(idx)
    }
    pub fn move_by(&self, scene: &mut Scene, x: f32, y: f32) {
        if let Some(square) = scene.get_mut(&self) {
            square.move_by(x, y);
        }
    }
    pub fn set_child(&mut self, scene: &mut Scene, child: &mut SquareIndex) {
        if let Some(sq) = scene.get_mut(&self) {
            sq.set_child(child.0);
        }
        if let Some(sq) = scene.get_mut(&child) {
            sq.set_parent(self.0);
        }
    }
    pub fn position(&self, scene: &Scene) -> Point {
        let pos = scene
            .get(&self)
            .map_or(Point::new(), |sq| sq.position.clone());

        let origin = scene
            .get_parent(&self)
            .map_or(Point::new(), |sq| sq.position.clone());

        pos + origin
    }
}

#[derive(Debug)]
pub struct Scene {
    store: Arena<Square>,
    objects: Vec<SquareIndex>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            store: Arena::new(),
            objects: vec![],
        }
    }
    pub fn add(&mut self, object: Square) -> SquareIndex {
        let sq = SquareIndex::new(self.store.insert(object));
        self.objects.push(sq);
        sq
    }
    pub fn get_mut(&mut self, sq: &SquareIndex) -> Option<&mut Square> {
        self.store.get_mut(sq.0)
    }
    pub fn get(&self, sq: &SquareIndex) -> Option<&Square> {
        self.store.get(sq.0)
    }
    pub fn get_parent(&self, sq: &SquareIndex) -> Option<&Square> {
        self.store
            .get(sq.0)
            .and_then(|sq| sq.parent.and_then(|parent| self.store.get(parent)))
    }
    pub fn draw(&mut self) {
        for obj in self.objects.iter() {
            println!("{:?}", obj.position(&self));
        }
    }
}

fn main() {
    let mut scene = Scene::new();

    let mut parent = scene.add(Square::new());
    let mut child = scene.add(Square::new());

    parent.set_child(&mut scene, &mut child);

    parent.move_by(&mut scene, 3.0, 3.0);
    child.move_by(&mut scene, 5.0, 5.0);

    dbg!(scene.get_parent(&child));

    scene.draw();

    // println!("{:?}", parent.position(&scene));
    // println!("{:?}", child.position(&scene));
}
