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
}

impl Square {
    pub fn new() -> Self {
        Self {
            position: Point::new(),
        }
    }
    pub fn move_by(&mut self, x: f32, y: f32) {
        self.position.x += x;
        self.position.y += y;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SquareIndex {
    pub inner: Index,
    pub parent: Option<Index>,
    pub child: Option<Index>,
}

impl SquareIndex {
    pub fn new(idx: Index) -> Self {
        Self {
            inner: idx,
            parent: None,
            child: None,
        }
    }
    pub fn move_by(&self, scene: &mut Scene, x: f32, y: f32) {
        if let Some(square) = scene.get_mut(self.inner) {
            square.move_by(x, y);
        }
    }
    pub fn set_child(&mut self, child: &mut SquareIndex) {
        self.child = Some(child.inner);
        child.parent = Some(self.inner);
    }
    pub fn position(&self, scene: &Scene) -> Point {
        let pos = scene
            .get(self.inner)
            .map_or(Point::new(), |sq| sq.position.clone());
        if let Some(parent) = self.parent {
            println!("Has parent");
            pos + scene
                .get(parent)
                .map_or(Point::new(), |sq| sq.position.clone())
        } else {
            pos
        }
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

    pub fn get_mut(&mut self, idx: Index) -> Option<&mut Square> {
        self.store.get_mut(idx)
    }

    pub fn get(&self, idx: Index) -> Option<&Square> {
        self.store.get(idx)
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

    parent.set_child(&mut child);

    parent.move_by(&mut scene, 3.0, 3.0);
    child.move_by(&mut scene, 5.0, 5.0);

    scene.draw();
    println!("{:?}", child.position(&scene));
}
