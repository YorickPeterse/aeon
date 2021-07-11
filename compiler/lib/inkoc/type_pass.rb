# frozen_string_literal: true

module Inkoc
  module TypePass
    def initialize(compiler, mod)
      @module = mod
      @state = compiler.state
      @constant_resolver = ConstantResolver.new(diagnostics)
    end

    def diagnostics
      @state.diagnostics
    end

    def typedb
      @state.typedb
    end

    def run(ast)
      locals = ast.locals
      scope = TypeScope
        .new(@module.type, @module.body.type, @module, locals: locals)

      on_module_body(ast, scope)

      [ast]
    end

    def on_module_body(node, scope)
      define_type(node, scope)
      nil
    end

    def on_body(node, scope)
      define_types(node.expressions, scope)
      nil
    end

    def on_constant(node, scope)
      @constant_resolver.resolve(node, scope)
    end

    def on_never_type(node, _)
      wrap_option_type(node, TypeSystem::Never.new)
    end

    def on_any_type(node, _)
      wrap_option_type(node, TypeSystem::Any.new)
    end

    def on_self_type_with_late_binding(node, _)
      wrap_option_type(node, TypeSystem::SelfType.new)
    end

    def on_self_type(node, scope)
      self_type = scope.self_type.as_owned

      # When "Self" translates to a generic type, e.g. Array!(T), we want to
      # return a type in the form of `Array!(T -> T)`, and not just `Array`.
      # This ensures that any arguments passed to a method returning "Self" can
      # properly initialise the type.
      type_arguments =
        self_type.generic_type? ? self_type.type_parameters.to_a : []

      wrap_option_type(node, self_type.new_instance(type_arguments))
    end

    def on_type_name(node, scope)
      type = define_type(node.name, scope)

      return type if type.error?

      unless type.generic_type?
        return wrap_option_type(node, type.new_instance)
      end

      # When our type is a generic type we need to initialise it according to
      # the passed type parameters.
      type_arguments = []

      node.type_parameters.zip(type.type_parameters) do |param_node, param|
        param_instance = define_type_instance(param_node, scope)

        if param && !param_instance.type_compatible?(param, @state)
          return diagnostics
            .type_error(param, param_instance, param_node.location)
        end

        type_arguments << param_instance
      end

      num_given = type_arguments.length
      num_expected = type.type_parameters.length

      if num_given != num_expected
        return diagnostics.type_parameter_count_error(
          num_given,
          num_expected,
          node.location
        )
      end

      # Simply referencing a constant should not lead to it being initialised,
      # unless there are any type parameters to initialise.
      wrap_option_type(
        node,
        type.new_instance_for_reference(type_arguments)
      )
    end

    def on_reference_type(node, scope)
      type = define_type_instance(node.type_node, scope)
      type = TypeSystem::Reference.new(type)

      wrap_option_type(node, type)
    end

    def define_type(node, scope, *extra)
      type = process_node(node, scope, *extra)

      node.type ||= type if type
    end

    def define_types(nodes, scope, *extra)
      nodes.map { |n| define_type(n, scope, *extra) }
    end

    def define_type_instance(node, scope, *extra)
      type = define_type(node, scope, *extra)

      if type && !type.type_instance?
        type = type.new_instance
        node.type = type
      end

      type
    end

    def wrap_option_type(node, type)
      return type unless node.optional?

      wrapped =
        if type.type_instance?
          type
        else
          type.new_instance
        end

      new_option_type_of(wrapped, node.location)
    end

    def store_type(type, scope, location)
      scope.self_type.base_type.define_attribute(type.name, type)
      store_type_as_global(type.name, type, scope, location)
      type
    end

    def store_type_as_global(name, type, scope, location)
      if Config::RESERVED_CONSTANTS.include?(name)
        diagnostics.redefine_reserved_constant_error(name, location)
      elsif scope.module_scope?
        @module.globals.define(name, type)
      end
    end

    def scope_for_object_body(node)
      TypeScope
        .new(node.type, node.block_type, @module, locals: node.body.locals)
    end

    def define_required_traits(node, trait, scope)
      node.required_traits.each do |req_node|
        req = define_type_instance(req_node, scope)

        trait.add_required_trait(req) unless req.error?
      end
    end

    def new_object_type
      @module.lookup_object_type&.new_instance
    end

    def new_option_type_of(type, location)
      if (type = @module.lookup_option_type&.new_instance([type]))
        type
      else
        diagnostics.undefined_constant_error(Config::OPTION_CONST, location)
      end
    end

    def on_raw_instruction(node, scope)
      callback = node.raw_instruction_visitor_method

      define_types(node.arguments, scope)

      if respond_to?(callback)
        public_send(callback, node, scope)
      else
        diagnostics.unknown_raw_instruction_error(node.name, node.location)

        TypeSystem::Error.new
      end
    end

    def on_raw_set_attribute(node, *)
      node.arguments.fetch(2).type
    end

    def on_raw_get_attribute(node, *)
      object = node.arguments.fetch(0).type
      name = node.arguments.fetch(1)

      if name.string?
        object.lookup_attribute(name.value).type
      else
        new_object_type
      end
    end
    alias on_raw_get_attribute_in_self on_raw_get_attribute

    def on_raw_allocate(node, *)
      if (proto = node.arguments[0]&.type)
        proto.new_instance
      else
        typedb.new_empty_object
      end
    end

    def on_raw_object_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_copy_blocks(*)
      TypeSystem::Never.new
    end

    def on_raw_integer_add(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_div(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_mul(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_sub(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_mod(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_and(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_or(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_xor(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_shift_left(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_shift_right(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_smaller(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_greater(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_greater_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_smaller_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_add(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_div(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_mul(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_sub(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_mod(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_smaller(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_greater(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_greater_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_smaller_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_get_true(*)
      typedb.true_type.new_instance
    end

    def on_raw_get_false(*)
      typedb.false_type.new_instance
    end

    def on_raw_get_nil(*)
      typedb.nil_type.new_instance
    end

    def on_raw_get_nil_prototype(*)
      typedb.nil_type
    end

    def on_raw_get_module_prototype(*)
      typedb.module_type
    end

    def on_raw_get_ffi_library_prototype(*)
      typedb.ffi_library_type
    end

    def on_raw_get_ffi_function_prototype(*)
      typedb.ffi_function_type
    end

    def on_raw_get_ffi_pointer_prototype(*)
      typedb.ffi_pointer_type
    end

    def on_raw_get_ip_socket_prototype(*)
      typedb.ip_socket_type
    end

    def on_raw_get_unix_socket_prototype(*)
      typedb.unix_socket_type
    end

    def on_raw_get_process_prototype(*)
      typedb.process_type
    end

    def on_raw_get_read_only_file_prototype(*)
      typedb.read_only_file_type
    end

    def on_raw_get_write_only_file_prototype(*)
      typedb.write_only_file_type
    end

    def on_raw_get_read_write_file_prototype(*)
      typedb.read_write_file_type
    end

    def on_raw_get_hasher_prototype(*)
      typedb.hasher_type
    end

    def on_raw_get_generator_prototype(*)
      typedb.generator_type
    end

    def on_raw_run_block(*)
      new_object_type
    end

    def on_raw_get_string_prototype(*)
      typedb.string_type
    end

    def on_raw_get_integer_prototype(*)
      typedb.integer_type
    end

    def on_raw_get_float_prototype(*)
      typedb.float_type
    end

    def on_raw_get_trait_prototype(*)
      typedb.trait_type
    end

    def on_raw_get_child_process_prototype(*)
      typedb.child_process_type
    end

    def on_raw_get_array_prototype(*)
      typedb.array_type
    end

    def on_raw_get_block_prototype(*)
      typedb.block_type
    end

    def on_raw_array_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_array_at(node, _)
      TypeSystem::Any.new
    end

    def on_raw_array_set(node, _)
      typedb.nil_type.new_instance
    end

    def on_raw_array_remove(node, _)
      TypeSystem::Any.new
    end

    def on_raw_string_size(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_string_concat(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_byte(*)
      typedb.integer_type.new_instance
    end

    def on_raw_process_spawn(node, _)
      typedb.process_type.new_instance
    end

    def on_raw_process_send_message(node, _)
      node.arguments.fetch(1).type
    end

    def on_raw_process_receive_message(node, *)
      new_object_type
    end

    def on_raw_process_current(node, _)
      typedb.process_type.new_instance
    end

    def on_raw_process_suspend_current(*)
      TypeSystem::Never.new
    end

    def on_raw_process_terminate_current(*)
      TypeSystem::Never.new
    end

    def on_raw_get_prototype(*)
      new_object_type
    end

    def on_raw_attribute_exists(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_close(*)
      typedb.nil_type.new_instance
    end

    def on_raw_process_set_blocking(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_panic(*)
      TypeSystem::Never.new
    end

    def on_raw_exit(*)
      TypeSystem::Never.new
    end

    def on_raw_byte_array_from_array(*)
      typedb.byte_array_type.new_instance
    end

    def on_raw_byte_array_set(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_at(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_remove(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_get_boolean_prototype(*)
      typedb.boolean_type
    end

    def on_raw_get_byte_array_prototype(*)
      typedb.byte_array_type
    end

    def on_raw_set_object_name(*)
      typedb.string_type.new_instance
    end

    def on_raw_current_file_path(*)
      typedb.string_type.new_instance
    end

    def on_raw_process_add_defer_to_caller(*)
      TypeSystem::Block.closure(typedb.block_type, return_type: new_object_type)
    end

    def on_raw_process_set_pinned(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_process_identifier(*)
      typedb.integer_type.new_instance
    end

    def on_raw_module_load(*)
      typedb.module_type.new_instance
    end

    def on_raw_module_get(*)
      typedb.module_type.new_instance
    end

    def on_raw_generator_resume(*)
      TypeSystem::Never.new
    end

    def on_raw_generator_value(*)
      TypeSystem::Any.new
    end

    def on_raw_get_null(*)
      TypeSystem::Any.new
    end

    def on_raw_to_iterator(node, scope)
      arg = node.arguments.fetch(0)

      move_if_variable(arg, scope)

      type = arg.type
      iter_type =
        @state.module(Config::ITER_MODULE)&.lookup_type(Config::ITER_TYPE)

      if iter_type.nil?
        diagnostics.for_unavailable(node.location)

        return TypeSystem::Error.new
      end

      if type.type_instance_of?(iter_type) ||
          type.implements_trait?(iter_type, @state)
        return type
      end

      iter_msg = Config::ITER_MESSAGE
      iter_method = type.lookup_method(iter_msg)

      if iter_method.nil?
        return diagnostics.undefined_method_error(type, iter_msg, node.location)
      end

      node.create_iterator = true
      node.block_type = iter_method.type

      iter_method.type.resolved_return_type(type)
    end
  end
end
