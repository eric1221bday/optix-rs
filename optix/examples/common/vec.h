#pragma once

#include "types.h"

#if __CUDACC__
#define DEVICE __device__
#else
#define DEVICE
#endif

namespace osc {

template <typename T> struct Vec2 {
    T x;
    T y;

    DEVICE Vec2(T x, T y) : x(x), y(y) {}
    DEVICE explicit Vec2(T v) : x(v), y(v) {}

    inline DEVICE auto operator[](size_t dim) -> T& { return (&x)[dim]; }
    inline DEVICE auto operator[](size_t dim) const -> const T& {
        return (&x)[dim];
    }

    inline DEVICE auto length2() const -> T { return x * x + y * y; }
    inline DEVICE auto length() const -> T { return sqrtf(length2()); }
};

template <typename T>
inline DEVICE auto operator+(const Vec2<T>& a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a.x + b.x, a.y + b.y);
}

template <typename T>
inline DEVICE auto operator-(const Vec2<T>& a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a.x - b.x, a.y - b.y);
}

template <typename T>
inline DEVICE auto operator*(const Vec2<T>& a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a.x * b.x, a.y * b.y);
}

template <typename T>
inline DEVICE auto operator/(const Vec2<T>& a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a.x / b.x, a.y / b.y);
}

template <typename T>
inline DEVICE auto operator+(T a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a + b.x, a + b.y);
}

template <typename T>
inline DEVICE auto operator-(T a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a - b.x, a - b.y);
}

template <typename T>
inline DEVICE auto operator*(T a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a * b.x, a * b.y);
}

template <typename T>
inline DEVICE auto operator/(T a, const Vec2<T>& b) -> Vec2<T> {
    return Vec2<T>(a / b.x, a / b.y);
}

template <typename T>
inline DEVICE auto operator+(const Vec2<T>& a, T b) -> Vec2<T> {
    return Vec2<T>(a.x + b, a.y + b);
}

template <typename T>
inline DEVICE auto operator-(const Vec2<T>& a, T b) -> Vec2<T> {
    return Vec2<T>(a.x - b, a.y - b);
}

template <typename T>
inline DEVICE auto operator*(const Vec2<T>& a, T b) -> Vec2<T> {
    return Vec2<T>(a.x * b, a.y * b);
}

template <typename T>
inline DEVICE auto operator/(const Vec2<T>& a, T b) -> Vec2<T> {
    return Vec2<T>(a.x / b, a.y / b);
}

template <typename T>
inline DEVICE auto normalize(const Vec2<T>& v) -> Vec2<T> {
    return v / v.length();
}

template <typename T>
inline DEVICE auto dot(const Vec2<T>& a, const Vec2<T>& b) -> Vec2<T> {
    return a.x * b.x + a.y * b.y;
}

using V2i32 = Vec2<i32>;
using V2f32 = Vec2<f32>;

/// Vec3
template <typename T> struct Vec3 {
    T x;
    T y;
    T z;

    DEVICE Vec3(T x, T y, T z) : x(x), y(y), z(z) {}
    DEVICE explicit Vec3(T v) : x(v), y(v), z(v) {}

    DEVICE operator float3() const { return make_float3(x, y, z); }

    inline DEVICE auto operator[](size_t dim) -> T& { return (&x)[dim]; }
    inline DEVICE auto operator[](size_t dim) const -> const T& {
        return (&x)[dim];
    }

    inline DEVICE auto length2() const -> T { return x * x + y * y + z * z; }
    inline DEVICE auto length() const -> T { return sqrtf(length2()); }
};

template <typename T>
inline DEVICE auto operator+(const Vec3<T>& a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a.x + b.x, a.y + b.y, a.z + b.z);
}

template <typename T>
inline DEVICE auto operator-(const Vec3<T>& a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a.x - b.x, a.y - b.y, a.z - b.z);
}

template <typename T>
inline DEVICE auto operator*(const Vec3<T>& a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a.x * b.x, a.y * b.y, a.z * b.z);
}

template <typename T>
inline DEVICE auto operator/(const Vec3<T>& a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a.x / b.x, a.y / b.y, a.z / b.z);
}

template <typename T>
inline DEVICE auto operator+(T a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a + b.x, a + b.y, a + b.z);
}

template <typename T>
inline DEVICE auto operator-(T a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a - b.x, a - b.y, a - b.z);
}

template <typename T>
inline DEVICE auto operator*(T a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a * b.x, a * b.y, a * b.z);
}

template <typename T>
inline DEVICE auto operator/(T a, const Vec3<T>& b) -> Vec3<T> {
    return Vec3<T>(a / b.x, a / b.y, a / b.z);
}

template <typename T>
inline DEVICE auto operator+(const Vec3<T>& a, T b) -> Vec3<T> {
    return Vec3<T>(a.x + b, a.y + b, a.z + b);
}

template <typename T>
inline DEVICE auto operator-(const Vec3<T>& a, T b) -> Vec3<T> {
    return Vec3<T>(a.x - b, a.y - b, a.z - b);
}

template <typename T>
inline DEVICE auto operator*(const Vec3<T>& a, T b) -> Vec3<T> {
    return Vec3<T>(a.x * b, a.y * b, a.z * b);
}

template <typename T>
inline DEVICE auto operator/(const Vec3<T>& a, T b) -> Vec3<T> {
    return Vec3<T>(a.x / b, a.y / b, a.z / b);
}

template <typename T>
inline DEVICE auto normalize(const Vec3<T>& v) -> Vec3<T> {
    return v / v.length();
}

template <typename T>
inline DEVICE auto dot(const Vec3<T>& a, const Vec3<T>& b) -> Vec3<T> {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

using V3f32 = Vec3<f32>;

} // namespace osc