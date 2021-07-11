# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Pass::DefineType do
  before do
    object_trait = state.typedb.new_trait_type(Inkoc::Config::OBJECT_CONST)

    tir_module.globals.define(Inkoc::Config::OBJECT_CONST, object_trait)

    state.typedb.nil_type.implement_trait(object_trait)
    state.typedb.integer_type.implement_trait(object_trait)
    state.typedb.string_type.implement_trait(object_trait)
  end

  shared_examples 'a Block type' do
    it 'returns a Block' do
      type = expression_type("#{header} {}")

      expect(type).to be_an_instance_of(Inkoc::TypeSystem::Block)
    end

    context 'when the method includes type parameters' do
      it 'defines the type parameters' do
        type = expression_type("#{header} !(A, B) {}")

        expect(type.type_parameters['A'])
          .to be_an_instance_of(Inkoc::TypeSystem::TypeParameter)

        expect(type.type_parameters['B'])
          .to be_an_instance_of(Inkoc::TypeSystem::TypeParameter)
      end

      it 'defines the required traits for the type parameters' do
        trait = state.typedb.new_trait_type('C')

        tir_module.type.define_attribute('C', trait)

        type = expression_type("#{header} !(A, B: C) {}")
        param_a = type.type_parameters['A']
        param_b = type.type_parameters['B']

        expect(param_a.required_traits).to be_empty
        expect(param_b.required_traits).to eq([trait])
      end

      it 'produces an error if a requirement is undefined' do
        type = expression_type("#{header} !(A, B: C) {}")

        expect(state.diagnostics.errors?).to eq(true)
        expect(type.type_parameters['A'].required_traits).to be_empty
        expect(type.type_parameters['B'].required_traits).to be_empty
      end

      it 'produces an error if a requirement is not a trait' do
        requirement = Inkoc::TypeSystem::Object.new(name: 'C')

        tir_module.type.define_attribute('C', requirement)

        type = expression_type("#{header} !(A, B: C) {}")

        expect(state.diagnostics.errors?).to eq(true)
        expect(type.type_parameters['A'].required_traits).to be_empty
        expect(type.type_parameters['B'].required_traits).to be_empty
      end

      it 'produces the correct return type when it uses the type parameters' do
        type_b = Inkoc::TypeSystem::Object.new(name: 'B')
        param_t = type_b.define_type_parameter('T')

        tir_module.type.define_attribute('B', type_b)

        type = expression_type("#{header} !(A) -> B!(A) {}")
        rtype = type.return_type
        param_a = type.lookup_type_parameter('A')

        expect(rtype).to be_type_instance_of(type_b)
        expect(rtype.lookup_type_parameter_instance(param_t)).to eq(param_a)
      end
    end

    context 'when defining arguments without types or default values' do
      it 'defines the arguments using an error type' do
        type = expression_type("#{header} (number) {}")
        symbol = type.arguments['number']

        expect(symbol).to be_any
        expect(symbol.type).to be_error
      end
    end

    context 'when defining arguments with a default value' do
      it 'uses the type of the default value as the argument type' do
        type = expression_type("#{header} (number = 10) {}")

        arg_type = type.arguments['number'].type

        expect(arg_type).to be_type_instance_of(state.typedb.integer_type)
        expect(arg_type).to be_type_instance
      end
    end

    context 'when defining arguments with an explicit type' do
      it 'uses the explicitly given type as the argument type' do
        tir_module.type.define_attribute('Integer', state.typedb.integer_type)

        type = expression_type("#{header} (number: Integer) {}")
        arg_type = type.arguments['number'].type

        expect(arg_type).to be_type_instance_of(state.typedb.integer_type)
        expect(arg_type).to be_type_instance
      end
    end

    context 'when both a default value and explicit type are given' do
      let(:number_trait) { state.typedb.new_trait_type('Number') }
      let(:integer_type) { state.typedb.integer_type }

      before do
        tir_module.type.define_attribute('Number', number_trait)
        tir_module.type.define_attribute('Integer', integer_type)
      end

      it 'produces a type error if the two types are not compatible' do
        type = expression_type("#{header} (number: Number = 10) {}")

        expect(state.diagnostics.errors?).to eq(true)

        expect(type.arguments['number'].type)
          .to be_type_instance_of(number_trait)
      end

      it 'uses the explicit type if compatible with the default value' do
        integer_type.implement_trait(number_trait)

        type = expression_type("#{header} (number: Number = 10) {}")

        expect(state.diagnostics.errors?).to eq(false)

        expect(type.arguments['number'].type)
          .to be_type_instance_of(number_trait)
      end
    end

    it 'defines a type to throw when a throw type is explicitly given' do
      int_type = state.typedb.integer_type

      tir_module.type.define_attribute('Integer', int_type)

      type = expression_type("#{header} !! Integer {}")

      expect(type.throw_type).to be_type_instance_of(int_type)
      expect(type.throw_type).to be_type_instance
    end

    it 'does not define a type to throw when no throw type is given' do
      type = expression_type("#{header} {}")

      expect(type.throw_type).to be_nil
    end

    it 'defines a type to return when a return type is explicitly given' do
      int_type = state.typedb.integer_type

      tir_module.type.define_attribute('Integer', int_type)

      type = expression_type("#{header} -> Integer {}")

      expect(type.return_type).to be_type_instance_of(int_type)
      expect(type.return_type).to be_type_instance
    end

    it 'defines the default return type when no return type is given' do
      type = expression_type("#{header} {}")

      expect(type.return_type).to be_type_instance_of(state.typedb.nil_type)
    end

    it 'defines a "call" method for the defined method' do
      type = expression_type("#{header} {}")

      expect(type.lookup_method('call')).to be_any
    end

    context 'when defining the "call" method' do
      let(:type) { expression_type("#{header} (a: A) !! B -> C {}") }
      let(:type_a) { Inkoc::TypeSystem::Object.new }
      let(:type_b) { Inkoc::TypeSystem::Object.new }
      let(:type_c) { Inkoc::TypeSystem::Object.new }
      let(:call_method) { type.lookup_method('call').type }

      before do
        tir_module.type.define_attribute('A', type_a)
        tir_module.type.define_attribute('B', type_b)
        tir_module.type.define_attribute('C', type_c)
      end

      it 'uses the same arguments as the method' do
        expect(call_method.arguments).to eq(type.arguments)
      end

      it 'uses the same throw type as the method' do
        expect(call_method.throw_type).to eq(type.throw_type)
      end

      it 'uses the same return type as the method' do
        expect(call_method.return_type).to eq(type.return_type)
      end
    end
  end

  shared_examples 'an anonymous block' do
    context 'when including arguments without explicit types' do
      it 'produces an error' do
        type = expression_type("#{header} (number) {}")

        arg_type = type.arguments['number'].type

        expect(arg_type).to be_error
      end

      it 'does not overwrite any explicitly defined types' do
        tir_module.type.define_attribute('Integer', state.typedb.integer_type)

        type = expression_type("#{header} (number: Integer) {}")
        arg_type = type.arguments['number'].type

        expect(arg_type).to be_type_instance_of(state.typedb.integer_type)
      end
    end

    it 'infers the return type based on the last expression' do
      type = expression_type("#{header} { 10 }")

      expect(type.return_type).to be_type_instance_of(state.typedb.integer_type)
    end

    it 'does not overwrite an explicitly defined return type' do
      type = expression_type("#{header} -> Object { 10 }")

      expect(type.return_type).to be_type_instance_of(tir_module.lookup_object_type)
    end

    context 'when the block includes a try statement without an else' do
      let(:throw_type) { Inkoc::TypeSystem::Object.new(name: 'Error') }

      before do
        foo_method = Inkoc::TypeSystem::Block
          .named_method('foo', state.typedb.block_type)

        foo_method.throw_type = throw_type

        type_scope.module_type.define_attribute('foo', foo_method)
      end

      it 'infers the throw type according to the try statement' do
        type = expression_type("#{header} { local try foo }")

        expect(type.throw_type).to be_type_instance_of(throw_type)
      end

      it 'does not overwrite an explicitly defined throw type' do
        type = expression_type("#{header} !! Object { try foo }")

        expect(type.throw_type)
          .to be_type_instance_of(tir_module.lookup_object_type)
      end
    end

    context 'when the block includes a try statement with an else' do
      it 'does not infer the throw type' do
        throw_type = Inkoc::TypeSystem::Object.new(name: 'Error')
        foo_method = Inkoc::TypeSystem::Block
          .named_method('foo', state.typedb.block_type)

        foo_method.throw_type = throw_type

        type_scope.self_type.define_attribute('foo', foo_method)

        type = expression_type("#{header} { try foo else nil }")

        expect(type.throw_type).to be_nil
      end
    end

    context 'when the block includes a throw statement' do
      let(:throw_type) { Inkoc::TypeSystem::Object.new(name: 'Error') }

      before do
        type_scope.module_type.define_attribute('Error', throw_type)
      end

      it 'infers the throw type according to the throw statement' do
        type = expression_type("#{header} { local throw Error }")

        expect(type.throw_type).to be_type_instance_of(throw_type)
      end

      it 'does not overwrite an explicitly defined throw type' do
        type = expression_type("#{header} !! Object { local throw Error }")

        expect(type.throw_type)
          .to be_type_instance_of(tir_module.lookup_object_type)
      end
    end
  end

  let(:tir_module) do
    new_tir_module.tap do |mod|
      mod.type = Inkoc::TypeSystem::Object.new
    end
  end

  let(:state) { Inkoc::State.new(Inkoc::Config.new) }
  let(:compiler) { Inkoc::Compiler.new(state) }
  let(:pass) { described_class.new(compiler, tir_module) }
  let(:self_type) { tir_module.type }

  let(:type_scope) do
    Inkoc::TypeScope.new(
      self_type,
      tir_module.body.type,
      tir_module,
      locals: Inkoc::SymbolTable.new
    )
  end

  def parse_expression(string)
    parse_source(string).expressions[0]
  end

  def expression_type(expression, scope = type_scope)
    node = parse_source(expression)

    Inkoc::Pass::SetupSymbolTables
      .new(compiler, tir_module)
      .run(node)

    Inkoc::Pass::DefineTypeSignatures
      .new(compiler, tir_module)
      .run(node)

    unless state.diagnostics.errors?
      Inkoc::Pass::ImplementTraits
        .new(compiler, tir_module)
        .run(node)

      pass.on_module_body(node, scope)
    end

    node.expressions[0].type
  end

  describe '#on_integer' do
    it 'returns the integer type' do
      type = expression_type('10')

      expect(type).to be_type_instance_of(state.typedb.integer_type)
    end
  end

  describe '#on_float' do
    it 'returns the float type' do
      type = expression_type('10.5')

      expect(type).to be_type_instance_of(state.typedb.float_type)
    end
  end

  describe '#on_string' do
    it 'returns the string type' do
      type = expression_type('"hello"')

      expect(type).to be_type_instance_of(state.typedb.string_type)
    end
  end

  describe '#on_constant' do
    it 'returns the type of a constant' do
      int_type = state.typedb.integer_type

      type_scope
        .self_type
        .define_attribute('A', int_type.new_instance)

      type = expression_type('A')

      expect(type).to be_type_instance_of(int_type)
    end

    it 'returns a type error for an undefined constant' do
      type = expression_type('A')

      expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_type_name' do
    def constant_type(type)
      node = parse_expression("let x: #{type} = 10").value_type

      pass.define_type(node, type_scope)
    end

    context 'when using an undefined constant' do
      it 'returns a TypeError::Error' do
        type = constant_type('A')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
      end

      it 'produces a type error' do
        type = constant_type('A')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    context 'when using a defined constant' do
      it 'returns the type of the constant' do
        type_a = Inkoc::TypeSystem::Object.new(name: 'A')

        tir_module.type.define_attribute('A', type_a)

        type = constant_type('A')

        expect(type).to eq(type_a)
      end
    end

    context 'when using a defined constant that accepts type parameters' do
      let(:trait) { state.typedb.new_trait_type('Number') }

      let(:object) do
        Inkoc::TypeSystem::Object.new.tap do |obj|
          obj.define_type_parameter('T', [trait])
        end
      end

      let(:integer_type) { state.typedb.integer_type }

      before do
        tir_module.type.define_attribute('A', object)
        tir_module.type.define_attribute('Integer', integer_type)
      end

      it 'initialises the type parameters' do
        integer_type.implement_trait(trait)

        type = constant_type('A!(Integer)')
        param = type.lookup_type_parameter('T')

        expect(type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(integer_type)
      end

      it 'initialises the type parameters when using a Self type' do
        param = type_scope.self_type.define_type_parameter('T')
        type = constant_type('Self')

        expect(type).to be_type_instance_of(type_scope.self_type)

        expect(type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(param)
      end

      it 'does not initialise the global type' do
        integer_type.implement_trait(trait)

        constant_type('A!(Integer)')

        expect(object.type_parameter_instances).to be_empty
      end

      it 'produces a type error if not enough type parameters are given' do
        integer_type.implement_trait(trait)

        type = constant_type('A!(Integer, Integer)')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'produces a type error if too many type parameters are given' do
        integer_type.implement_trait(trait)

        type = constant_type('A')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'produces a type error if the type parameter is not compatible' do
        type = constant_type('A!(Integer)')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end
    end
  end

  describe '#on_attribute' do
    context 'when using an existing attribute' do
      it 'returns the type of the attribute' do
        object = Inkoc::TypeSystem::Object.new

        type_scope.self_type.define_attribute('@number', object)
        type_scope.define_variable_state('@number')

        type = expression_type('@number')

        expect(type).to eq(object)
      end
    end

    context 'when using a non-existing attribute' do
      it 'produces a type error' do
        type = expression_type('@number')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(state.diagnostics.errors?).to eq(true)
      end
    end
  end

  describe '#on_identifier' do
    context 'when using an undefined identifier' do
      it 'produces a type error' do
        type = expression_type('foo')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    context 'when the identifier is a local variable' do
      it 'returns the type of the local variable' do
        object = Inkoc::TypeSystem::Object.new

        type_scope.define_local('foo', object)

        type = expression_type('foo')

        expect(type).to eq(object)
      end

      it 'supports the use of method bounds' do
        allow(type_scope.block_type)
          .to receive(:method?)
          .and_return(true)

        trait = state.typedb.new_trait_type('Inspect')
        param = type_scope.self_type.define_type_parameter('T')

        type_scope.block_type.method_bounds.define('T', [trait])
        type_scope.define_local('foo', param)

        type = expression_type('foo')

        expect(type).to be_type_parameter
        expect(type.required_traits).to include(trait)
      end
    end

    context 'when the identifier is a module method' do
      it 'returns the return type of the method' do
        object = Inkoc::TypeSystem::Object.new
        method = Inkoc::TypeSystem::Block.new(
          name: 'foo',
          block_type: Inkoc::TypeSystem::Block::METHOD,
          return_type: object
        )

        tir_module.type.define_attribute('foo', method)

        node = parse_expression('foo')
        type = pass.define_type(node, type_scope)

        expect(type).to be_type_instance_of(object)
        expect(node.block_type).to be_type_instance_of(method)
      end
    end

    context 'when the identifier is a method defined on self' do
      it 'returns the return type of the method' do
        method_return_type = Inkoc::TypeSystem::Object.new(name: 'A')
        method = Inkoc::TypeSystem::Block.new(
          name: 'foo',
          block_type: Inkoc::TypeSystem::Block::METHOD,
          return_type: method_return_type
        )

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo')

        expect(type).to be_type_instance_of(method_return_type)
      end

      it 'initialises any type parameters in the return type' do
        method_return_type = Inkoc::TypeSystem::Object.new(name: 'A')

        param = type_scope.self_type.define_type_parameter('T')
        param_instance = Inkoc::TypeSystem::Object.new(name: 'B')

        method_param = method_return_type.define_type_parameter('A')

        method_return_type.initialize_type_parameter(method_param, param)

        method = Inkoc::TypeSystem::Block.new(
          name: 'foo',
          block_type: Inkoc::TypeSystem::Block::METHOD,
          return_type: method_return_type
        )

        type_scope.self_type.initialize_type_parameter(param, param_instance)
        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo')

        expect(type).to be_type_instance_of(method_return_type)

        expect(type.lookup_type_parameter_instance(method_param))
          .to be_type_instance_of(param_instance)
      end
    end

    context 'when the identifier is a local variable shadowing a method' do
      it 'returns the type of the local variable' do
        method_return_type = Inkoc::TypeSystem::Object.new(name: 'A')
        variable_type = Inkoc::TypeSystem::Object.new(name: 'B')
        method = Inkoc::TypeSystem::Block.new(
          name: 'foo',
          block_type: Inkoc::TypeSystem::Block::METHOD,
          return_type: method_return_type
        )

        tir_module.type.define_attribute('foo', method)
        type_scope.define_local('foo', variable_type)

        type = expression_type('foo')

        expect(type).to eq(variable_type)
      end
    end

    context 'when the identifier is a global variable' do
      it 'returns the type of the global variable' do
        object = Inkoc::TypeSystem::Object.new

        tir_module.globals.define('foo', object)

        type = expression_type('foo')

        expect(type).to eq(object)
      end
    end

    context 'when the identifier is a local variable shadowing a global' do
      it 'returns the type of the local variable' do
        local_type = Inkoc::TypeSystem::Object.new(name: 'A')
        global_type = Inkoc::TypeSystem::Object.new(name: 'B')

        tir_module.globals.define('foo', global_type)
        type_scope.define_local('foo', local_type)

        type = expression_type('foo')

        expect(type).to eq(local_type)
      end
    end
  end

  describe '#on_send' do
    context 'when not enough arguments are given' do
      it 'produces a type error' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')

        method.define_required_argument('foo', state.typedb.integer_type)

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo()')

        expect(state.diagnostics.errors?).to eq(true)
        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
      end
    end

    context 'when too many arguments are given' do
      it 'produces a type error' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(10)')

        expect(state.diagnostics.errors?).to eq(true)
        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
      end
    end

    context 'when the method takes no arguments and none are given' do
      it 'returns the return type of the method' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        rtype = Inkoc::TypeSystem::Object.new(name: 'A')

        method.return_type = rtype

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo()')

        expect(type).to be_type_instance_of(rtype)
      end
    end

    context 'when the method takes arguments and enough are given' do
      it 'returns the return type of the method' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        rtype = Inkoc::TypeSystem::Object.new(name: 'A')

        method.return_type = rtype
        method.define_required_argument(
          'thing',
          state.typedb.integer_type.new_instance
        )

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(10)')

        expect(type).to be_type_instance_of(rtype)
      end

      it 'resolves the return type using the instances of the receiver' do
        receiver = Inkoc::TypeSystem::Object.new(name: 'Receiver')
        param = receiver.define_type_parameter('T')
        int_type = state.typedb.integer_type
        method = Inkoc::TypeSystem::Block.new(name: 'foo', return_type: param)

        receiver.define_attribute(method.name, method)
        receiver.initialize_type_parameter(param, int_type)

        type_scope.define_local('receiver', receiver)

        type = expression_type('receiver.foo')

        expect(type).to be_type_instance_of(int_type)
      end

      it 'produces a type error when a given argument is not compatible' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        rtype = Inkoc::TypeSystem::Object.new(name: 'A')

        method.return_type = rtype
        method.define_required_argument('thing', state.typedb.integer_type)

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(10.5)')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'initialises any method type parameters' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param = method.define_type_parameter('T')

        # foo!(T)(thing: T) -> T
        method.define_required_argument('thing', param)
        method.return_type = param

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(10)')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(method.type_parameter_instances).to be_empty
      end

      it 'initialises any method type parameters using a keyword argument' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param1 = method.define_type_parameter('A')
        param2 = method.define_type_parameter('B')

        method.define_required_argument('first', param1)
        method.define_required_argument('second', param2)

        method.return_type = param2

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(second: 20, first: 10.5)')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(method.type_parameter_instances).to be_empty
      end

      it 'does not re-initialise method type parameters using a keyword' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param = method.define_type_parameter('A')

        method.define_required_argument('first', param)
        method.define_required_argument('second', param)

        method.return_type = param

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(first: 10, second: 10.5)')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
        expect(method.type_parameter_instances).to be_empty
      end

      it 'does not re-initialise instance type parameters using a keyword' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param = type_scope.self_type.define_type_parameter('A')
        int_type = state.typedb.integer_type

        method.define_required_argument('first', param)
        type_scope.self_type.initialize_type_parameter(param, int_type)

        method.return_type = param

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(first: 10.5)')

        expect(type).to be_error
        expect(method.type_parameter_instances).to be_empty
      end

      it 'initialises any instance type parameters' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param = type_scope.self_type.define_type_parameter('T')
        int_type = state.typedb.integer_type

        # foo(thing: T) -> T
        method.define_required_argument('thing', param)
        method.return_type = param

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('foo(10)')

        expect(type).to be_type_instance_of(int_type)
        expect(method.type_parameter_instances).to be_empty

        expect(self_type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(int_type)
      end

      it 'does not initialise already initialised type parameters' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')
        param = type_scope.self_type.define_type_parameter('T')
        int_type = state.typedb.integer_type

        # foo(thing: T) -> T
        method.define_required_argument('thing', param)
        method.return_type = param

        type_scope.self_type.define_attribute('foo', method)
        type_scope.self_type.initialize_type_parameter(param, int_type)

        type = expression_type('foo(10.5)')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)

        expect(self_type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(int_type)
      end

      it 'does not initialize type parameters using uninitialized generics' do
        foo_method = Inkoc::TypeSystem::Block.new(name: 'foo')
        trait = state.typedb.new_trait_type('Equal')
        method_param = foo_method.define_type_parameter('MethodParam', [trait])

        foo_method.define_required_argument(
          'values',
          state.typedb.new_array_of_type(method_param)
        )

        foo_method.return_type = state.typedb.new_array_of_type(method_param)

        type_scope.self_type.define_attribute(foo_method.name, foo_method)
        type_scope.define_local('list', state.typedb.array_type.new_instance)

        type = expression_type('foo(list)')

        expect(type).to be_type_instance_of(state.typedb.array_type)

        param = type.lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

        # T should not be initialised because the input `list` type does not
        # contain any initialised parameters that can be used to initialise T.
        expect(type.lookup_type_parameter_instance(param)).to be_nil
      end
    end

    context 'when the method defines a rest argument' do
      let(:block) do
        Inkoc::TypeSystem::Block
          .new(name: 'foo', return_type: tir_module.lookup_object_type.new_instance)
      end

      before do
        type_scope.self_type.define_attribute('foo', block)
      end

      it 'supports passing of excessive arguments' do
        rest_type = state.typedb.new_array_of_type(state.typedb.integer_type)

        block.define_rest_argument('rest', rest_type)

        type = expression_type('foo(10, 20, 30)')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)
        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'validates excessive arguments according to the rest argument' do
        rest_type = state.typedb.new_array_of_type(state.typedb.integer_type)

        block.define_rest_argument('rest', rest_type)

        type = expression_type('foo(10, 20, 30.5)')

        expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'supports the use of a generic rest argument type' do
        param = block.define_type_parameter('T')
        rest_type = state.typedb.new_array_of_type(param)

        block.define_rest_argument('rest', rest_type)
        block.return_type = param

        exp = state.typedb.integer_type
        type = expression_type('foo(10, 20, 30)')

        expect(type).to be_type_instance_of(exp)
      end
    end

    context 'when passing a closure as an argument' do
      def parse_closure_argument(expr)
        body = parse_source(expr)

        Inkoc::Pass::SetupSymbolTables
          .new(compiler, tir_module)
          .run(body)

        send_node = body.expressions[0]
        send_arg = send_node.arguments[0]

        pass.define_type(body, type_scope)

        [send_node.type, send_arg.type]
      end

      let(:integer_type) { state.typedb.integer_type }
      let(:expected_block) do
        Inkoc::TypeSystem::Block.new(
          return_type: tir_module.lookup_object_type.new_instance,
          throw_type: tir_module.lookup_object_type.new_instance
        )
      end

      let(:method) do
        Inkoc::TypeSystem::Block
          .new(name: 'foo', return_type: tir_module.lookup_object_type.new_instance)
      end

      before do
        method.define_required_argument('callback', expected_block)

        type_scope.self_type.define_attribute('foo', method)
      end

      it 'resolves type parameters in the expected closure' do
        param = type_scope.self_type.define_type_parameter('T')
        int_type = state.typedb.integer_type

        expected_block.return_type = param

        type_scope
          .self_type
          .initialize_type_parameter(param, int_type.new_instance)

        parse_closure_argument('foo do () { 10.5 }')

        # This should produce an error diagnostic because:
        #
        # 1. The return type of our given closure is a Float.
        # 2. The expected block's return type is T, which is initialised as
        #    Integer.
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'infers the arguments of the closure' do
        expected_block.define_required_argument('foo', integer_type)

        type, closure = parse_closure_argument('foo do (thing) { }')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)

        expect(closure).to be_instance_of(Inkoc::TypeSystem::Block)

        expect(closure.arguments['thing'].type)
          .to be_type_instance_of(state.typedb.integer_type)
      end

      it 'infers the arguments of a generic closure' do
        expected_block.define_required_argument('foo', integer_type)

        param = type_scope.self_type.define_type_parameter('T')
        float_type = state.typedb.float_type

        type_scope.self_type.initialize_type_parameter(param, float_type)

        expected_block.define_required_argument('bar', param)

        _, closure = parse_closure_argument('foo do (thing, bar) { }')

        expect(closure.arguments['bar'].type)
          .to be_type_instance_of(float_type)
      end

      it 'infers the return type' do
        expected_block.define_required_argument('foo', integer_type)

        type, closure = parse_closure_argument('foo do (thing) { thing }')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)

        expect(closure.return_type)
          .to be_type_instance_of(state.typedb.integer_type)
      end

      it 'infers the throw type' do
        expected_block.define_required_argument('foo', integer_type)

        type, closure =
          parse_closure_argument('foo do (thing) { local throw thing }')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)

        expect(closure.throw_type)
          .to be_type_instance_of(state.typedb.integer_type)
      end

      it 'can infer a closure without arguments as a lambda' do
        expected_block.block_type = Inkoc::TypeSystem::Block::LAMBDA

        type, closure = parse_closure_argument('foo {}')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)
        expect(closure).to be_lambda
      end

      it 'does not error when an inferred lambda shadows a local variable' do
        expected_block.block_type = Inkoc::TypeSystem::Block::LAMBDA

        type_scope.define_local('a', state.typedb.integer_type)

        type, closure = parse_closure_argument('foo { let a = 10 }')

        expect(type).to be_type_instance_of(tir_module.lookup_object_type)
        expect(closure).to be_lambda
        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'allows use of passed type arguments in the expected block' do
        param = method.define_type_parameter('T')

        expected_block.define_required_argument(
          'foo',
          state.typedb.new_array_of_type(param)
        )

        tir_module.globals.define('Integer', state.typedb.integer_type)

        _, closure = parse_closure_argument('foo!(Integer) do (arg) {}')

        arg = closure.arguments['arg'].type
        array_param = state.typedb.array_type
          .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

        expect(arg).to be_type_instance_of(state.typedb.array_type)

        expect(arg.lookup_type_parameter_instance(array_param))
          .to be_type_instance_of(state.typedb.integer_type)
      end
    end

    it 'supports the use of keyword arguments' do
      method = Inkoc::TypeSystem::Block
        .new(name: 'foo', return_type: tir_module.lookup_object_type.new_instance)

      method.define_required_argument(
        'thing',
        state.typedb.integer_type.new_instance
      )

      type_scope.self_type.define_attribute('foo', method)

      type = expression_type('foo(thing: 10)')

      expect(type).to be_type_instance_of(tir_module.lookup_object_type)
      expect(state.diagnostics.errors?).to eq(false)
    end

    it 'produces a type error when using an invalid keyword argument' do
      method = Inkoc::TypeSystem::Block.new(name: 'foo')

      method.define_required_argument(
        'thing',
        state.typedb.integer_type.new_instance
      )

      type_scope.self_type.define_attribute('foo', method)

      type = expression_type('foo(foo: 10)')

      expect(type).to be_an_instance_of(Inkoc::TypeSystem::Error)
      expect(state.diagnostics.errors?).to eq(true)
    end

    context 'when sending a message to an explicit receiver' do
      it 'returns the type of the message' do
        method = Inkoc::TypeSystem::Block.new(name: 'foo')

        method.return_type = state.typedb.integer_type

        type_scope.self_type.define_attribute('foo', method)

        type = expression_type('self.foo')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
      end
    end

    context 'when sending "new" to a constant' do
      it 'does not initialize parameters in the original object' do
        a_type = Inkoc::TypeSystem::Object.new(name: 'A')
        method = Inkoc::TypeSystem::Block.new(name: 'A')
        param = a_type.define_type_parameter('T')

        method.define_required_argument('foo', param)
        method.return_type = a_type.new_instance([param])

        a_type.define_attribute('new', method)
        type_scope.self_type.define_attribute('A', a_type)

        type = expression_type('A.new(10)')

        expect(a_type.type_parameter_instances).to be_empty
        expect(type).to be_type_instance_of(a_type)

        expect(type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(state.typedb.integer_type)
      end
    end

    context 'when sending a message to a Error type' do
      it 'returns an Error type' do
        type = expression_type('foo.bar')

        expect(type).to be_error
      end
    end

    context 'when using a method with method bounds' do
      it 'does not error if the method bounds are met' do
        param = type_scope.self_type.define_type_parameter('T')
        method = Inkoc::TypeSystem::Block.new(name: 'to_string')
        trait = state.typedb.new_trait_type('ToString')
        int_type = state.typedb.integer_type

        int_type.implement_trait(trait)

        method.method_bounds.define(param.name, [trait])
        method.return_type = int_type.new_instance

        type_scope.self_type.initialize_type_parameter(param, int_type)
        type_scope.self_type.define_attribute(method.name, method)

        type = expression_type('to_string()')

        expect(type).to be_type_instance_of(int_type)
        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'errors if the method bounds are not met' do
        param = type_scope.self_type.define_type_parameter('T')
        method = Inkoc::TypeSystem::Block.new(name: 'to_string')
        trait = state.typedb.new_trait_type('ToString')
        int_type = state.typedb.integer_type

        method.method_bounds.define(param.name, [trait])
        method.return_type = int_type.new_instance

        type_scope.self_type.initialize_type_parameter(param, int_type)
        type_scope.self_type.define_attribute(method.name, method)

        type = expression_type('to_string()')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    it 'stores the method type in the AST node' do
      method = Inkoc::TypeSystem::Block.new(name: 'foo')

      type_scope.self_type.define_attribute('foo', method)

      node = parse_expression('foo()')

      pass.define_type(node, type_scope)

      expect(node.block_type).to be_type_instance_of(method)
    end

    context 'when passing explicit type arguments' do
      it 'initialises the type arguments when valid' do
        method = Inkoc::TypeSystem::Block
          .named_method('foo', state.typedb.block_type)

        param = method.define_type_parameter('T')

        method.return_type = param

        tir_module.globals.define('Integer', state.typedb.integer_type)
        type_scope.self_type.define_attribute(method.name, method)

        type = expression_type('foo!(Integer)')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
      end

      it 'errors when too many type arguments are given' do
        method = Inkoc::TypeSystem::Block
          .named_method('foo', state.typedb.block_type)

        param = method.define_type_parameter('T')

        method.return_type = param

        tir_module.globals.define('Integer', state.typedb.integer_type)
        tir_module.globals.define('Float', state.typedb.float_type)

        type_scope.self_type.define_attribute(method.name, method)

        type = expression_type('foo!(Integer, Float)')

        expect(type).to be_error
      end
    end
  end

  describe '#on_body' do
    shared_examples 'inside an anonymous block' do
      it 'infers the return type of the block' do
        type = expression_type("#{keyword} { 10 }")

        expect(type.return_type)
          .to be_type_instance_of(state.typedb.integer_type)
      end

      it 'does not overwrite explicitly defined return types' do
        type = expression_type("#{keyword} -> Object { 10 }")

        expect(type.return_type)
          .to be_type_instance_of(tir_module.lookup_object_type)
      end

      it 'produces a type error if the expression is not compatible' do
        type_scope.module_type
          .define_attribute('Integer', state.typedb.integer_type)

        type = expression_type("#{keyword} -> Integer { 10.5 }")

        expect(type.return_type)
          .to be_type_instance_of(state.typedb.integer_type)

        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    it 'returns the type of the last expression' do
      node = parse_source("10\n10.5")
      type = pass.define_type(node, type_scope)

      expect(type).to be_type_instance_of(state.typedb.float_type)
    end

    it 'returns nil if the body is empty' do
      node = parse_source('')
      type = pass.define_type(node, type_scope)

      expect(type).to be_type_instance_of(state.typedb.nil_type)
    end

    context 'when inside a closure' do
      let(:keyword) { 'do' }

      include_examples 'inside an anonymous block'
    end

    context 'when inside a lambda' do
      let(:keyword) { 'fn' }

      include_examples 'inside an anonymous block'
    end
  end

  describe '#on_return' do
    context 'when used inside a method' do
      it 'returns a Never type' do
        # An explicit return statement is a block return, thus the body that
        # contains it won't return anything (unless the body is in a method).
        body = parse_source('def foo { return 10 }')
        ret_node = body.expressions[0].body.expressions.last

        Inkoc::Pass::SetupSymbolTables
          .new(compiler, tir_module)
          .run(body)

        pass.define_type(body, type_scope)
        pass.process_deferred_methods

        expect(ret_node.type).to be_never
      end

      it 'does not produce any errors when the return type is compatible' do
        tir_module.globals.define('Integer', state.typedb.integer_type)
        expression_type('def foo -> Integer { return 10 }')

        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'produces an error if the return type is not compatible' do
        type_scope.module_type
          .define_attribute('Integer', state.typedb.integer_type)

        expression_type('def foo -> Integer { return }')

        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    context 'when used outside a method' do
      it 'produces an error' do
        type = expression_type('return 10')

        expect(type).to be_never
        expect(state.diagnostics.errors?).to eq(true)
      end
    end
  end

  describe '#on_try' do
    let(:method) do
      Inkoc::TypeSystem::Block.named_method('foo', state.typedb.block_type)
    end

    before do
      method.throw_type = state.typedb.integer_type
      method.return_type = state.typedb.nil_type

      type_scope.self_type.define_attribute('throws', method)
    end

    context 'with an else statement' do
      it 'returns the type of the try expression' do
        type = expression_type('do { try throws else nil }')

        expect(type.return_type)
          .to be_type_instance_of(state.typedb.nil_type)
      end

      it 'validates the else type according to the try return type' do
        type = expression_type('do { try throws else 10.5 }')

        expect(type.return_type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'defines the type of the error argument' do
        body = parse_source('do { try throws else (error) error }')

        Inkoc::Pass::SetupSymbolTables
          .new(compiler, tir_module)
          .run(body)

        pass.define_type(body, type_scope)

        try_node = body.expressions[0].body.expressions[0]
        error_local = try_node.else_body.locals['error']
        error_arg = try_node.else_block_type.arguments['error']

        expect(error_local.type)
          .to be_type_instance_of(state.typedb.integer_type)

        expect(error_arg.type)
          .to be_type_instance_of(state.typedb.integer_type)
      end

      it 'defines the error argument as a Object type if no error is thrown' do
        body = parse_source('do { try 10 else (error) error }')

        Inkoc::Pass::SetupSymbolTables
          .new(compiler, tir_module)
          .run(body)

        pass.define_type(body, type_scope)

        try_node = body.expressions[0].body.expressions[0]
        error_local = try_node.else_body.locals['error']

        expect(error_local.type).to be_instance_of(Inkoc::TypeSystem::Any)
      end

      it 'allows referencing of outer local variables in the else block' do
        body = parse_source(<<~SOURCE)
          let number = 10

          do { try 10 else { number } }
        SOURCE

        Inkoc::Pass::SetupSymbolTables
          .new(compiler, tir_module)
          .run(body)

        type_scope = Inkoc::TypeScope.new(
          self_type,
          tir_module.body.type,
          tir_module,
          locals: body.locals
        )

        pass.on_module_body(body, type_scope)

        expect(state.diagnostics.errors?).to eq(false)
      end
    end

    context 'without an else statement' do
      it 'produces a warning if the expression never throws' do
        type = expression_type('local try 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(state.diagnostics.warnings?).to eq(true)
      end

      it 'infers the throw type of a surrounding closure' do
        type = expression_type('do { local try throws }')

        expect(type.throw_type)
          .to be_type_instance_of(state.typedb.integer_type)
      end
    end

    context 'when using try!' do
      it 'returns the type of the expression' do
        type = expression_type('try! 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(state.diagnostics.warnings?).to eq(false)
      end
    end
  end

  describe '#on_throw' do
    it 'returns a never type' do
      type = expression_type('throw 10')

      expect(type).to be_never
    end

    it 'infers the throw type of the surrounding closure' do
      type = expression_type('do { local throw 10 }')

      expect(type.throw_type).to be_type_instance_of(state.typedb.integer_type)
    end

    it 'does not infer the throw type at the top-level' do
      expression_type('throw 10')

      expect(type_scope.block_type.throw_type).to be_nil
    end
  end

  describe '#on_object' do
    it 'defines an object using its name' do
      type = expression_type('class Person {}')

      expect(type).to be_instance_of(Inkoc::TypeSystem::Object)
      expect(type.name).to eq('Person')
      expect(type.prototype).to be_nil
    end

    it 'produces a type error when using a reserved constant' do
      expression_type('class Self {}')

      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'defines the name attribute of the object' do
      type = expression_type('class Person {}')
      attr = Inkoc::Config::OBJECT_NAME_INSTANCE_ATTRIBUTE

      expect(type.lookup_attribute(attr).type)
        .to be_type_instance_of(state.typedb.string_type)
    end

    it 'defines the object in the current scope' do
      type = expression_type('class Person {}')

      expect(type_scope.self_type.lookup_attribute('Person').type).to eq(type)
    end

    it 'defines the object as a global when defined in the module scope' do
      scope = Inkoc::TypeScope.new(
        tir_module.type,
        Inkoc::TypeSystem::Block.new,
        tir_module,
        locals: Inkoc::SymbolTable.new
      )

      type = expression_type('class Person {}', scope)

      expect(tir_module.lookup_global('Person')).to eq(type)
    end

    it 'defines any type parameters the object may have' do
      trait = state.typedb.new_trait_type('C')

      type_scope.module_type.define_attribute('C', trait)

      type = expression_type('class Person!(A, B: C) {}')

      param_a = type.lookup_type_parameter('A')
      param_b = type.lookup_type_parameter('B')

      expect(param_a.required_traits).to be_empty
      expect(param_b.required_traits).to include(trait)
    end

    it 'supports type parameters with generic requirements' do
      trait = state.typedb.new_trait_type('B')
      param = trait.define_type_parameter('T')

      type_scope.module_type.define_attribute('B', trait)

      type_scope
        .module_type
        .define_attribute('Integer', state.typedb.integer_type)

      type = expression_type('class Person!(A: B!(Integer)) {}')

      required_trait = type
        .lookup_type_parameter('A')
        .required_traits
        .first

      expect(required_trait).to be_type_instance_of(trait)

      expect(required_trait.lookup_type_parameter_instance(param))
        .to be_type_instance_of(state.typedb.integer_type)
    end

    it 'supports defining methods on the object' do
      type = expression_type('class Person { def foo {} }')
      method = type.lookup_method('foo')

      expect(method.type).to be_method
      expect(method.type.name).to eq('foo')
    end
  end

  describe '#on_trait' do
    it 'defines a trait using its name' do
      type = expression_type('trait Person {}')

      expect(type).to be_instance_of(Inkoc::TypeSystem::Trait)
      expect(type.name).to eq('Person')
    end

    it 'produces a type error when using a reserved constant' do
      expression_type('trait Self {}')

      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'defines the name attribute of the trait' do
      type = expression_type('trait Person {}')
      attr = Inkoc::Config::OBJECT_NAME_INSTANCE_ATTRIBUTE

      expect(type.lookup_attribute(attr).type)
        .to be_type_instance_of(state.typedb.string_type)
    end

    it 'defines the trait in the current scope' do
      type = expression_type('trait Person {}')

      expect(type_scope.self_type.lookup_attribute('Person').type).to eq(type)
    end

    it 'defines the trait as a global when defined in the module scope' do
      scope = Inkoc::TypeScope.new(
        tir_module.type,
        Inkoc::TypeSystem::Block.new,
        tir_module,
        locals: Inkoc::SymbolTable.new
      )

      type = expression_type('trait Person {}', scope)

      expect(tir_module.lookup_global('Person')).to eq(type)
    end

    it 'defines any type parameters the trait may have' do
      trait = state.typedb.new_trait_type('C')

      type_scope.module_type.define_attribute('C', trait)

      type = expression_type('trait Person!(A, B: C) {}')

      param_a = type.lookup_type_parameter('A')
      param_b = type.lookup_type_parameter('B')

      expect(param_a.required_traits).to be_empty
      expect(param_b.required_traits).to include(trait)
    end

    it 'supports type parameters with generic requirements' do
      trait = state.typedb.new_trait_type('B')
      param = trait.define_type_parameter('T')

      type_scope.module_type.define_attribute('B', trait)

      type_scope
        .module_type
        .define_attribute('Integer', state.typedb.integer_type)

      type = expression_type('trait Person!(A: B!(Integer)) {}')

      required_trait = type
        .lookup_type_parameter('A')
        .required_traits
        .first

      expect(required_trait).to be_type_instance_of(trait)

      expect(required_trait.lookup_type_parameter_instance(param))
        .to be_type_instance_of(state.typedb.integer_type)
    end

    it 'supports defining methods on the trait' do
      type = expression_type('trait Person { def foo {} }')
      method = type.lookup_method('foo')

      expect(method.type).to be_method
      expect(method.type.name).to eq('foo')
    end

    it 'supports defining required methods on the trait' do
      type = expression_type('trait Person { def foo }')
      method = type.required_methods['foo']

      expect(method.type).to be_method
      expect(method.type.name).to eq('foo')
    end

    it 'supports defining of required traits' do
      to_string = state.typedb.new_trait_type('ToString')

      type_scope.self_type.define_attribute(to_string.name, to_string)

      type = expression_type('trait Inspect: ToString {}')

      expect(type.implements_trait?(to_string, state)).to eq(true)
    end

    it 'errors when extending a non-empty trait' do
      to_string = state.typedb.new_trait_type('ToString')
      inspect = state.typedb.new_trait_type('Inspect')

      inspect.add_required_trait(to_string.new_instance)

      type_scope.self_type.define_attribute(inspect.name, inspect)
      type_scope.self_type.define_attribute(to_string.name, to_string)

      type = expression_type('trait Inspect: ToString { def inspect {} }')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_block' do
    let(:header) { 'do' }

    it_behaves_like 'a Block type'
    it_behaves_like 'an anonymous block'

    it 'produces a closure' do
      type = expression_type('do {}')

      expect(type).to be_closure
    end
  end

  describe '#on_lambda' do
    let(:header) { 'fn' }

    it_behaves_like 'a Block type'
    it_behaves_like 'an anonymous block'

    it 'produces a lambda' do
      type = expression_type('fn {}')

      expect(type).to be_lambda
    end

    it 'defines the type of self as the module type' do
      type = expression_type('fn { self }')

      expect(type.return_type).to be_type_instance_of(tir_module.type)
    end
  end

  describe '#on_method' do
    let(:header) { 'def foo' }

    it_behaves_like 'a Block type'

    it 'produces a method' do
      type = expression_type('def foo {}')

      expect(type).to be_method
    end

    it 'supports defining of method bounds' do
      inspect = state.typedb.new_trait_type('Inspect')
      to_string = state.typedb.new_trait_type('ToString')

      type_scope
        .self_type
        .define_attribute(inspect.name, inspect)

      type_scope
        .self_type
        .define_attribute(to_string.name, to_string)

      type_scope
        .self_type
        .define_type_parameter('T', [to_string])

      type = expression_type('def foo(value: T) when T: Inspect {}')

      bound = type.method_bounds['T']

      expect(bound).to be_type_parameter
      expect(bound.required_traits).to include(inspect, to_string)
    end

    it 'errors when using a type bound with a method type parameter' do
      inspect = state.typedb.new_trait_type('Inspect')

      type_scope
        .self_type
        .define_attribute(inspect.name, inspect)

      expression_type('def foo!(T)(value: T) when T: Inspect {}')

      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'supports the use of a method bound' do
      inspect = state.typedb.new_trait_type('Inspect')

      type_scope
        .self_type
        .define_attribute(inspect.name, inspect)

      type_scope
        .self_type
        .define_type_parameter('T')

      type = expression_type('def foo(value: T) when T: Inspect { value }')
      arg = type.arguments['value'].type

      expect(arg).to be_type_parameter
      expect(arg.required_traits).to include(inspect)
      expect(state.diagnostics.errors?).to eq(false)
    end

    # rubocop: disable RSpec/ExampleLength
    it 'remaps return values of other methods when using a method bound' do
      inspect = state.typedb.new_trait_type('Inspect')

      inspect_method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      inspect_method.return_type = state
        .typedb
        .string_type
        .new_instance

      inspect.define_attribute(inspect_method.name, inspect_method)

      param = type_scope
        .self_type
        .define_type_parameter('T')

      foo = Inkoc::TypeSystem::Block
        .named_method('foo', state.typedb.block_type)

      foo.return_type = param

      type_scope
        .self_type
        .define_attribute(inspect.name, inspect)

      type_scope
        .self_type
        .define_attribute(foo.name, foo)

      node = parse_source('def bar when T: Inspect { foo.inspect }')

      Inkoc::Pass::SetupSymbolTables
        .new(compiler, tir_module)
        .run(node)

      pass.on_module_body(node, type_scope)

      foo_send = node
        .expressions[0]
        .body
        .expressions[0]
        .receiver

      expect(state.diagnostics.errors?).to eq(false)

      expect(foo_send.type).to be_type_parameter
      expect(foo_send.type.required_traits).to include(inspect)
    end
    # rubocop: enable RSpec/ExampleLength

    it 'supports returning Self in a generic object' do
      type_scope.self_type.define_type_parameter('T')

      type = expression_type('def foo -> Self {}')

      expect(type).to be_method
      expect(type.return_type).to be_self_type
    end

    it 'supports returning Never' do
      type = expression_type('def foo -> Never {}')

      expect(type).to be_method
      expect(type.return_type).to be_never
    end
  end

  describe '#on_required_method' do
    it 'defines a required method' do
      expression_type('trait Foo { def foo }')

      trait = type_scope.self_type.lookup_attribute('Foo').type

      expect(trait.required_methods['foo']).to be_any
    end

    it 'produces a type error when used outside of a trait' do
      expression_type('def foo')

      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_define_variable' do
    context 'with a local variable' do
      it 'defines the local variable' do
        type = expression_type('let x = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)

        expect(type_scope.locals['x'].type).to eq(type)
      end

      it 'defines a mutable local variable' do
        expression_type('let mut x = 10')

        expect(type_scope.locals['x']).to be_mutable
      end

      it 'errors if the local variable already exists' do
        type_scope.define_local('a', state.typedb.integer_type)

        type = expression_type('let a = 10')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    context 'with a constant' do
      it 'defines the constant' do
        type = expression_type('let X = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)

        attr = type_scope.self_type.attributes['X']

        expect(attr.type).to eq(type)
      end

      it 'errors if the constant already exists' do
        type_scope.self_type.define_attribute('A', state.typedb.integer_type)

        type = expression_type('let A = 10')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'defines the constant as a global if defined at the top-level' do
        scope = Inkoc::TypeScope.new(
          tir_module.type,
          tir_module.body.type,
          tir_module,
          locals: Inkoc::SymbolTable.new
        )

        type = expression_type('let X = 10', scope)

        expect(tir_module.globals['X'].type).to eq(type)
      end
    end
  end

  describe '#on_define_variable_with_explicit_type' do
    context 'with a local variable' do
      it 'defines the local variable' do
        type_scope
          .self_type
          .define_attribute('Integer', state.typedb.integer_type)

        type = expression_type('let x: Integer = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)

        local = type_scope.locals['x']

        expect(local).to be_any
        expect(local.type).to eq(type)
      end

      it 'errors if the value and explicit type are not compatible' do
        type_scope
          .module_type
          .define_attribute('Float', state.typedb.float_type)

        type = expression_type('let x: Float = 10')

        expect(type).to be_error
        expect(type_scope.locals['x'].type).to be_error
      end

      it 'supports the use of a generic type' do
        type_scope.self_type.define_attribute('Array', state.typedb.array_type)

        type_scope
          .self_type
          .define_attribute('Integer', state.typedb.integer_type)

        type = expression_type('let x: Array!(Integer) = Array.new(10)')

        param = state
          .typedb
          .array_type
          .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

        expect(type).to be_type_instance_of(state.typedb.array_type)

        expect(type.lookup_type_parameter_instance(param))
          .to be_type_instance_of(state.typedb.integer_type)

        expect(state.typedb.array_type.type_parameter_instances).to be_empty
      end
    end

    context 'with a constant' do
      it 'defines the constant' do
        type_scope
          .self_type
          .define_attribute('Integer', state.typedb.integer_type)

        type = expression_type('let X: Integer = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)

        attr = type_scope.self_type.attributes['X']

        expect(attr).to be_any
        expect(attr.type).to eq(type)
      end

      it 'errors if the value and explicit type are not compatible' do
        type_scope
          .module_type
          .define_attribute('Float', state.typedb.float_type)

        type = expression_type('let X: Float = 10')

        expect(type).to be_error
        expect(type_scope.self_type.attributes['X'].type).to be_error
      end
    end
  end

  describe '#on_reassign_variable' do
    context 'with a local variable' do
      it 'reassigns a mutable local variable with a compatible type' do
        type_scope
          .define_local('number', state.typedb.integer_type, true)

        type = expression_type('number = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'errors if the local variable is not mutable' do
        type_scope
          .define_local('number', state.typedb.integer_type.new_instance, false)

        type = expression_type('number = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'errors if the local variable is not defined' do
        type = expression_type('number = 10')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'errors if the new type is not compatible with the old one' do
        type_scope
          .define_local('number', state.typedb.float_type, true)

        type = expression_type('number = 10')

        expect(type).to be_type_instance_of(state.typedb.float_type)

        expect(type_scope.locals['number'].type)
          .to be_type_instance_of(state.typedb.float_type)

        expect(state.diagnostics.errors?).to eq(true)
      end
    end

    context 'with an attribute' do
      it 'reassigns a mutable attribute with a compatible type' do
        type_scope
          .self_type
          .attributes
          .define('@number', state.typedb.integer_type, true)

        type_scope.define_variable_state('@number')

        type = expression_type('@number = 10')

        expect(type).to be_type_instance_of(state.typedb.integer_type)
        expect(state.diagnostics.errors?).to eq(false)
      end

      it 'errors if the attribute is not defined' do
        type = expression_type('@number = 10')

        expect(type).to be_error
        expect(state.diagnostics.errors?).to eq(true)
      end

      it 'errors if the new type is not compatible with the old one' do
        type_scope
          .self_type
          .attributes
          .define('@number', state.typedb.float_type, true)

        type_scope.define_variable_state('@number')

        type = expression_type('@number = 10')

        expect(type).to be_type_instance_of(state.typedb.float_type)

        expect(type_scope.self_type.attributes['@number'].type)
          .to be_type_instance_of(state.typedb.float_type)

        expect(state.diagnostics.errors?).to eq(true)
      end
    end
  end

  describe '#on_reopen_object' do
    it 'errors when using an undefined object' do
      type = expression_type('impl Foo {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'reopens an existing object' do
      object = Inkoc::TypeSystem::Object.new(name: 'Foo')

      type_scope
        .self_type
        .define_attribute('Foo', object)

      type = expression_type('impl Foo { def foo {} }')

      expect(type).to eq(object)
      expect(object.lookup_method('foo')).to be_any
    end

    it 'reopens an existing generic object' do
      object = Inkoc::TypeSystem::Object.new(name: 'Foo')

      object.define_type_parameter('T')

      type_scope
        .self_type
        .define_attribute('Foo', object)

      type = expression_type('impl Foo { def foo {} }')

      expect(type).to eq(object)
      expect(object.lookup_method('foo')).to be_any
    end

    it 'errors when reopening a trait' do
      trait = state.typedb.new_trait_type('Foo')

      type_scope
        .self_type
        .define_attribute('Foo', trait)

      type = expression_type('impl Foo {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_type_cast' do
    it 'errors when using an invalid cast' do
      type = expression_type('10 as Float')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_trait_implementation' do
    let(:object) { Inkoc::TypeSystem::Object.new(name: 'List') }
    let(:trait) { state.typedb.new_trait_type('Inspect') }

    before do
      type_scope
        .module_type
        .define_attribute(object.name, object)

      type_scope
        .module_type
        .define_attribute(trait.name, trait)
    end

    it 'errors if a required method is not implemented' do
      method = Inkoc::TypeSystem::Block
        .named_method('foo', state.typedb.block_type)

      trait.define_required_method(method)

      type = expression_type('impl Inspect for List {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'errors if a required trait is not implemented' do
      to_string = state.typedb.new_trait_type('ToString')

      trait.add_required_trait(to_string.new_instance)

      type = expression_type('impl Inspect for List {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'errors if the trait does not exist' do
      type = expression_type('impl Foo for List {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'errors if the object does not exist' do
      type = expression_type('impl Inspect for Foo {}')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end

    it 'implements a trait if all requirements are met' do
      type = expression_type('impl Inspect for List {}')

      expect(type).to be_type_instance_of(trait)
      expect(object.implements_trait?(trait)).to eq(true)
      expect(state.diagnostics.errors?).to eq(false)
    end

    it 'supports implementing of a generic trait' do
      param = trait.define_type_parameter('T')
      method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      method.return_type = param

      trait.define_required_method(method)

      type_scope
        .module_type
        .define_attribute('String', state.typedb.string_type)

      type = expression_type(
        'impl Inspect!(String) for List { def inspect -> String { "" } }'
      )

      expect(type).to be_type_instance_of(trait)
      expect(state.diagnostics.errors?).to eq(false)

      expect(type.lookup_type_parameter_instance(param))
        .to be_type_instance_of(state.typedb.string_type)

      expect(object.implements_trait?(type)).to eq(true)
    end

    it 'errors if a generic trait is not implemented correctly' do
      param = trait.define_type_parameter('T')
      method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      method.return_type = param

      trait.define_required_method(method)

      type_scope
        .module_type
        .define_attribute('String', state.typedb.string_type)

      type_scope
        .module_type
        .define_attribute('Integer', state.typedb.integer_type)

      type = expression_type(
        'impl Inspect!(String) for List { def inspect -> Integer { "" } }'
      )

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
      expect(object.implemented_traits).to be_empty
    end

    it 'supports the use of Self in the trait name' do
      param = trait.define_type_parameter('T')
      method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      trait.define_required_method(method)

      type_scope
        .module_type
        .define_attribute('String', state.typedb.string_type)

      type = expression_type('impl Inspect!(Self) for List { def inspect {} }')

      expect(type).to be_type_instance_of(trait)
      expect(state.diagnostics.errors?).to eq(false)

      expect(type.lookup_type_parameter_instance(param))
        .to be_type_instance_of(object)

      expect(object.implements_trait?(type)).to eq(true)
    end
  end

  describe '#on_global' do
    it 'returns the type of a global variable' do
      int_type = state
        .typedb
        .integer_type

      tir_module
        .globals
        .define('number', int_type.new_instance)

      type = expression_type('::number')

      expect(type).to be_type_instance_of(int_type)
    end

    it 'errors if a global variable does not exist' do
      type = expression_type('::number')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_raw_instruction' do
    it 'returns the type of a raw instruction' do
      type = expression_type('_INKOC.get_true')

      expect(type).to be_type_instance_of(state.typedb.boolean_type)
    end

    it 'errors for an unknown raw instruction' do
      type = expression_type('_INKOC.foo')

      expect(type).to be_error
      expect(state.diagnostics.errors?).to eq(true)
    end
  end

  describe '#on_raw_set_attribute' do
    it 'returns the type of the value' do
      object = Inkoc::TypeSystem::Object.new
      value = Inkoc::TypeSystem::Object.new(name: 'Value')

      type_scope.define_local('obj', object)
      type_scope.define_local('value', value)

      type = expression_type('_INKOC.set_attribute(obj, "name", value)')

      expect(type).to eq(value)
    end
  end

  describe '#on_raw_get_attribute' do
    let(:attribute) { Inkoc::TypeSystem::Object.new(name: 'Attribute') }

    before do
      object = Inkoc::TypeSystem::Object.new

      object.define_attribute('attribute', attribute)

      type_scope.define_local('obj', object)
    end

    it 'returns the type of an attribute when using a string literal' do
      type = expression_type('_INKOC.get_attribute(obj, "attribute")')

      expect(type).to eq(attribute)
    end

    it 'returns a Object type when not using a string literal' do
      type_scope
        .define_local('attribute', state.typedb.string_type.new_instance)

      type = expression_type('_INKOC.get_attribute(obj, attribute)')

      expect(type).to be_type_instance_of(tir_module.lookup_object_type)
    end
  end

  describe '#on_raw_allocate' do
    it 'returns a new object' do
      type = expression_type('_INKOC.allocate')

      expect(type).to be_object
    end

    it 'sets the prototype if given' do
      proto = state.typedb.integer_type

      type_scope.define_local('proto', proto)

      type = expression_type('_INKOC.allocate(proto)')

      expect(type.prototype).to eq(proto)
      expect(type).to be_type_instance_of(proto)
    end
  end

  describe '#on_self' do
    it 'returns the type of self' do
      type = expression_type('self')

      expect(type).to be_type_instance_of(type_scope.self_type)
    end
  end

  describe '#on_block_type' do
    def constant_type(type, scope = type_scope)
      node = parse_expression("let x: #{type} = 10").value_type

      pass.define_type(node, scope)
    end

    let(:integer) { state.typedb.integer_type }
    let(:float) { state.typedb.float_type }
    let(:string) { state.typedb.string_type }

    before do
      type_scope.self_type.define_attribute(integer.name, integer)
      type_scope.self_type.define_attribute(float.name, float)
      type_scope.self_type.define_attribute(string.name, string)
    end

    it 'returns a new closure type' do
      type = constant_type('do (Integer) !! Float -> String')

      expect(type).to be_closure
      expect(type.arguments['0'].type).to be_type_instance_of(integer)
      expect(type.throw_type).to be_type_instance_of(float)
      expect(type.return_type).to be_type_instance_of(string)
    end

    it 'returns a new lambda type' do
      type = constant_type('fn (Integer) !! Float -> String')

      expect(type).to be_lambda
      expect(type.arguments['0'].type).to be_type_instance_of(integer)
      expect(type.throw_type).to be_type_instance_of(float)
      expect(type.return_type).to be_type_instance_of(string)
    end

    it 'allows the use of type parameters defined in an enclosing method' do
      method_type = Inkoc::TypeSystem::Block
        .named_method('foo', state.typedb.block_type)

      param = method_type.define_type_parameter('Foo')
      scope = Inkoc::TypeScope.new(
        self_type,
        tir_module.body.type,
        tir_module,
        locals: Inkoc::SymbolTable.new,
        enclosing_method: method_type
      )

      array_type = state.typedb.array_type
      array_param = array_type
        .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      tir_module.globals.define('Array', array_type)

      type = constant_type('fn (Array!(Foo))', scope)
      arg_type = type.arguments['0'].type

      expect(state.diagnostics.errors?).to eq(false)

      expect(arg_type).to be_type_instance_of(array_type)
      expect(arg_type.lookup_type_parameter_instance(array_param))
        .to be_type_instance_of(param)
    end
  end

  describe 'array literals' do
    let(:array_type) { state.typedb.array_type }

    before do
      tir_module.globals.define(Inkoc::Config::ARRAY_CONST, array_type)

      new_method = Inkoc::TypeSystem::Block
        .named_method(Inkoc::Config::NEW_MESSAGE, state.typedb.block_type)

      param = new_method.define_type_parameter('V')

      new_method
        .define_rest_argument('values', state.typedb.new_array_of_type(param))

      new_method.return_type = state.typedb.new_array_of_type(param)

      array_type.define_attribute('new', new_method)
    end

    it 'returns the type of an empty Array' do
      type = expression_type('Array.new')
      array_type = state.typedb.array_type

      param =
        array_type.lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      expect(type).to be_type_instance_of(array_type)
      expect(type.lookup_type_parameter_instance(param)).to be_nil
    end

    it 'returns the type of an Array of Strings' do
      type = expression_type('Array.new("hello")')

      array_type = state.typedb.array_type

      param =
        array_type.lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      expect(type).to be_type_instance_of(array_type)

      expect(type.lookup_type_parameter_instance(param))
        .to be_type_instance_of(state.typedb.string_type)
    end

    it 'returns the type of an Array of Integers' do
      type = expression_type('Array.new(10)')

      array_type = state.typedb.array_type

      param =
        array_type.lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      expect(type).to be_type_instance_of(array_type)

      expect(type.lookup_type_parameter_instance(param))
        .to be_type_instance_of(state.typedb.integer_type)
    end
  end
end
