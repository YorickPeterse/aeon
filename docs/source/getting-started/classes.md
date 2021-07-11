# Classes

We've briefly covered classes in the [Basic types](basic-types.md) chapter. In
this chapter we'll take a closer look at them.

In Inko, classes don't support inheritance. Instead, behaviour is reused through
composition by using traits.

## Defining classes

You can define a class by using the `class` keyword, followed by its name and a
pair of curly braces:

```inko
class Person {

}
```

Instances of classes are created as follows:

```inko
Person {}
```

We refer to this pattern as a "constructor", because it's used to construct a
new instance of a class.

## Attributes

Of course our empty class is not useful, so let's give it some fields to store
data in. We call such fields "attributes", and we define them in the body of an
class. All attributes must be defined in the class before they can be used.
Let's say we want our `Person` type to have two attributes: a name and an age
attribute. Our name will be a `String`, and the age will be an `Integer`. We can
do this as follows:

```inko
class Person {
  @name: String
  @age: Integer
}
```

We refer to attributes using the syntax `@NAME` where `NAME` is the name of the
attribute. Attributes are private to the class, meaning they can't be accessed
directly. Instead, you must define a method that returns the attribute, which
we'll cover below.

When defining an attribute as done above, we can't specify a default value for
the attribute; instead it's up to the user of our type to assign a value to all
attributes. For our `Person` example above, this is done as follows:

```inko
Person { @name = 'Alice', @age = 32 }
```

When creating a class instance, all attributes must be assigned. If an attribute
is not assigned, a compile-time error is produced.

## Methods

Classes can have two types of methods: instance methods, and static methods.
Instance methods are only available to instances of the class, while static
methods are available to the class itself.

To define an instance method, use the `def` keyword:

```inko
class Person {
  @name: String
  @age: Integer

  def name -> String {
    @name
  }
}
```

Here we define the instance method `name`, which returns the `@name` attribute.
We can use this method like so:

```inko
Person { @name = 'Alice', @age = 42 }.name # => 'Alice'
```

To define a static method, use `static def`:

```inko
class Person {
  @name: String
  @age: Integer

  static def anonymous(age: Integer) -> Person {
    Person { @name = 'Anonymous', @age = age }
  }

  def name -> String {
    @name
  }
}
```

We can then use it like so:

```inko
Person.anonymous(42).name # => 'Anonymous'
```

Static methods can't refer to attributes, meaning that this is an error:

```inko
class Person {
  @name: String
  @age: Integer

  static def oops -> String {
    @name
  }
}
```

Both static and instance methods can use `self` to refer to their receiver. In
case of an instance method, that will be the instance the method is called on (a
`Person` instance for our `name` instance method). For static methods, this will
be the class itself (`Person` in this case). You don't need to use `self` if
you want to send a message to the current receiver. This means that this:

```inko
class Person {
  @name: String
  @age: Integer

  def nickname -> String {
    self.name
  }

  def name -> String {
    @name
  }
}
```

Is the same as this:

```inko
class Person {
  @name: String
  @age: Integer

  def nickname -> String {
    name
  }

  def name -> String {
    @name
  }
}
```

Sometimes you _do_ need to use `self`. For example, if a method takes an
argument with the same name as another method.

## Constructor methods

Having to specify all attributes when creating class instances can be tedious.
To make it easier to create instances, you can use what's known as a constructor
method. A constructor method is a static method used to create a new instance of
the class it's defined on. We saw such an example earlier: the static
`anonymous` method used to create a new `Person`.

Various types come with at least one constructor method: `new`. For example,
instances of `Array` are created using such a method:

```inko
Array.new(10, 20, 30)
```

For our `Person` example shown earlier, we can define a `new` method like so:

```inko
class Person {
  @name: String
  @age: Integer

  static def new(name: String, age: Integer) -> Self {
    Person { @name = name, @age = age }
  }
}
```

We can then create an instance as follows:

```inko
Person.new(name: 'Alice', age: 32)
```

Having to repeat the type name in our constructor method is a bit tedious.
Instead of doing this, we can use the `Self` type when constructing a class
instance:

```inko
class Person {
  @name: String
  @age: Integer

  static def new(name: String, age: Integer) -> Self {
    Self { @name = name, @age = age }
  }
}
```

The use of `Self` for a constructor is only valid in a static method.

When defining classes, we recommend defining at least a static `new` method for
the class. Various built-in types even require the use of `new` to create an
instance, and don't support the use of the constructor syntax. These types are
as follows:

* `Array`
* `Block`
* `Boolean`
* `ByteArray`
* `Float`
* `Integer`
* `Module`
* `NilType`
* `String`
* `std::ffi::Function`
* `std::ffi::Library`
* `std::ffi::Pointer`
* `std::fs::file::ReadOnlyFile`
* `std::fs::file::ReadWriteFile`
* `std::fs::file::WriteOnlyFile`
* `std::hash::Hasher`
* `std::net::Socket`
* `std::process::Process`
* `std::unix::Socket`

## Reopening classes

A class can be reopened in any module, allowing you to add new methods after
its initial definition. This is done as follows:

```inko
class Person {
  @name: String
}

impl Person {
  def name -> String {
    @name
  }
}
```

Here we reopen `Person`, and add the `name` instance method to it.
