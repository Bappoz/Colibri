use std::ops::{Add, Mul, Neg, Sub};

// =======================================================================
//                              Vec3d
//

#[derive(Debug, Clone, Default, Copy)]
pub struct Vec3d {
    v: [f64; 3],
}

impl Vec3d {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { v: [x, y, z] }
    }

    pub fn x(&self) -> f64 {
        self.v[0]
    }

    pub fn y(&self) -> f64 {
        self.v[1]
    }

    pub fn z(&self) -> f64 {
        self.v[2]
    }

    pub fn dot_product(&self, other: &Vec3d) -> f64 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z()
    }

    pub fn cross(&self, other: &Vec3d) -> Vec3d {
        Vec3d::new(
            self.y() * other.z() - self.z() * other.y(),
            self.z() * other.x() - self.x() * other.z(),
            self.x() * other.y() - self.y() * other.x(),
        )
    }

    pub fn length(&self) -> f64 {
        self.dot_product(self).sqrt()
    }

    pub fn normalize(&self) -> Vec3d {
        *self * (1.0 / self.length())
    }
}

impl Add for Vec3d {
    type Output = Vec3d;
    fn add(self, other: Vec3d) -> Vec3d {
        Vec3d::new(
            self.x() + other.x(),
            self.y() + other.y(),
            self.z() + other.z(),
        )
    }
}

impl Sub for Vec3d {
    type Output = Vec3d;
    fn sub(self, other: Vec3d) -> Vec3d {
        Vec3d::new(
            self.x() - other.x(),
            self.y() - other.y(),
            self.z() - other.z(),
        )
    }
}

impl Mul<f64> for Vec3d {
    type Output = Vec3d;
    fn mul(self, scalar: f64) -> Vec3d {
        Vec3d::new(self.x() * scalar, self.y() * scalar, self.z() * scalar)
    }
}

impl Neg for Vec3d {
    type Output = Vec3d;
    fn neg(self) -> Vec3d {
        Vec3d::new(-self.x(), -self.y(), -self.z())
    }
}

// =================================================================================
//                                     Vec4d
//

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4d {
    v: [f64; 4],
}

impl Vec4d {
    pub fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { v: [x, y, z, w] }
    }

    pub fn x(&self) -> f64 {
        self.v[0]
    }
    pub fn y(&self) -> f64 {
        self.v[1]
    }
    pub fn z(&self) -> f64 {
        self.v[2]
    }
    pub fn w(&self) -> f64 {
        self.v[3]
    }

    fn get(&self, i: usize) -> f64 {
        self.v[i]
    }

    pub fn dot(&self, other: &Vec4d) -> f64 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z() + self.w() * other.w()
    }
}

// =================================================================================
//                                     Mat4x4
//

#[derive(Debug, Copy, Clone)]
pub struct Mat4x4 {
    m: [Vec4d; 4],
}

impl Mat4x4 {
    pub fn identity() -> Self {
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, 1.0, 0.0, 0.0),
                Vec4d::new(0.0, 0.0, 1.0, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    fn col(&self, j: usize) -> Vec4d {
        Vec4d::new(
            self.m[0].get(j),
            self.m[1].get(j),
            self.m[2].get(j),
            self.m[3].get(j),
        )
    }

    // Matriz de projeção de perpectiva -- Usarei o padrão da OpenGl (right-handed e -Z pra frente)
    pub fn perpective(fov_y_radians: f64, aspect: f64, near: f64, far: f64) -> Self {
        let f = 1.0 / (fov_y_radians / 2.0).tan();
        Self {
            m: [
                Vec4d::new(f / aspect, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, f, 0.0, 0.0),
                Vec4d::new(
                    0.0,
                    0.0,
                    (far + near) / (near - far),
                    (2.0 * far * near) / (near - far),
                ),
                Vec4d::new(0.0, 0.0, -1.0, 0.0),
            ],
        }
    }

    pub fn translation(t: Vec3d) -> Self {
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, t.x()),
                Vec4d::new(0.0, 1.0, 0.0, t.y()),
                Vec4d::new(0.0, 0.0, 1.0, t.z()),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn scale(s: Vec3d) -> Self {
        Self {
            m: [
                Vec4d::new(s.x(), 0.0, 0.0, 0.0),
                Vec4d::new(0.0, s.y(), 0.0, 0.0),
                Vec4d::new(0.0, 0.0, s.z(), 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn rotation_x(radians: f64) -> Self {
        let (s, c) = radians.sin_cos(); // um só cálculo pra seno e cosseno
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, c, -s, 0.0),
                Vec4d::new(0.0, s, c, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn rotation_y(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                Vec4d::new(c, 0.0, s, 0.0),
                Vec4d::new(0.0, 1.0, 0.0, 0.0),
                Vec4d::new(-s, 0.0, c, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn rotation_z(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                Vec4d::new(c, -s, 0.0, 0.0),
                Vec4d::new(s, c, 0.0, 0.0),
                Vec4d::new(0.0, 0.0, 1.0, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Responsável por criar a matriz "câmera":
    /// Para onde a camera olha (target)
    /// a partir de "Pos" usando "up" como referência
    pub fn point_at(pos: Vec3d, target: Vec3d, up: Vec3d) -> Self {
        let forward = (target - pos).normalize();
        let a = forward * up.dot_product(&forward);
        let up = (up - a).normalize();

        let right = up.cross(&forward);
        Self {
            m: [
                Vec4d::new(right.x(), up.x(), forward.x(), pos.x()),
                Vec4d::new(right.y(), up.y(), forward.y(), pos.y()),
                Vec4d::new(right.z(), up.z(), forward.z(), pos.z()),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Inverte "rapidamente" uma matriz rotação + translação.
    /// Transpõe bloco 3x3 e racalcula a transalação.
    /// !!! só funciona para esse tipo de matriz (Ortonormal)
    pub fn quick_inverse(&self) -> Self {
        let right = Vec3d::new(self.m[0].x(), self.m[1].x(), self.m[2].x());
        let up = Vec3d::new(self.m[0].y(), self.m[1].y(), self.m[2].y());
        let forward = Vec3d::new(self.m[0].z(), self.m[1].z(), self.m[2].z());
        let pos = Vec3d::new(self.m[0].w(), self.m[1].w(), self.m[2].w());

        Self {
            m: [
                Vec4d::new(right.x(), right.y(), right.z(), -right.dot_product(&pos)),
                Vec4d::new(up.x(), up.y(), up.z(), -up.dot_product(&pos)),
                Vec4d::new(
                    forward.x(),
                    forward.y(),
                    forward.z(),
                    -forward.dot_product(&pos),
                ),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }
}

impl Mul<Vec4d> for Mat4x4 {
    type Output = Vec4d;
    fn mul(self, v: Vec4d) -> Vec4d {
        Vec4d::new(
            self.m[0].dot(&v),
            self.m[1].dot(&v),
            self.m[2].dot(&v),
            self.m[3].dot(&v),
        )
    }
}

impl Mul<Mat4x4> for Mat4x4 {
    type Output = Mat4x4;
    fn mul(self, rhs: Mat4x4) -> Mat4x4 {
        let mut result = [Vec4d::new(0.0, 0.0, 0.0, 0.0); 4];
        for i in 0..4 {
            let row = self.m[i];
            result[i] = Vec4d::new(
                row.dot(&rhs.col(0)),
                row.dot(&rhs.col(1)),
                row.dot(&rhs.col(2)),
                row.dot(&rhs.col(3)),
            );
        }
        Mat4x4 { m: result }
    }
}
