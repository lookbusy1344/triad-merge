// sample.cpp exercises the C++ grammar's highlight captures.
#include <string>
#include <vector>

#define LIMIT 10

/*
 * A block comment.
 *
 * It spans a blank line.
 */

namespace shapes {

enum class Colour { Red, Green = 2, Blue };

template <typename T>
concept Numeric = requires(T a) { a + a; };

class Shape {
public:
    explicit Shape(std::string name) : name_(std::move(name)) {}
    virtual ~Shape() = default;
    virtual double area() const noexcept = 0;
    const std::string &name() const { return name_; }

private:
    std::string name_;
};

}  // namespace shapes

class Square final : public shapes::Shape {
public:
    Square(double side) : Shape("square"), side_(side) {}
    double area() const noexcept override { return side_ * side_; }

private:
    double side_ = 0.0;
};

template <shapes::Numeric T>
constexpr T square(T x) {
    return x * x;
}

auto total(const std::vector<shapes::Shape *> &all) -> double {
    double sum = 0;
    for (const auto *shape : all) {
        if (shape == nullptr) {
            continue;
        }
        sum += shape->area();
    }
    return sum;
}

int main() {
    auto unit = new Square(2.5);
    const char *raw = R"(a "raw" string)";
    try {
        std::vector<shapes::Shape *> all{unit};
        return total(all) > LIMIT ? 0x1F : square(3);
    } catch (const std::exception &error) {
        delete unit;
        throw;
    }
}
