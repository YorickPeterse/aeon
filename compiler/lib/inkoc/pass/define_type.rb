# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class DefineType
      include VisitorMethods
      include TypePass

      DeferredMethod = Struct.new(:node, :scope)

      def initialize(compiler, mod)
        super(compiler, mod)

        @deferred_methods = []
        @loop_nesting = 0
      end

      def enter_loop
        @loop_nesting += 1
        retval = yield
        @loop_nesting -= 1

        retval
      end

      def inside_loop?
        @loop_nesting.positive?
      end

      def process_deferred_methods
        @deferred_methods.each do |method|
          on_deferred_method(method.node, method.scope)
        end
      end

      def on_module_body(node, scope)
        type = define_type(node, scope)

        process_deferred_methods

        type
      end

      def on_integer(node, _)
        max = (1 << 64) - 1

        diagnostics.integer_too_large(node.location) if node.value > max
        typedb.integer_type.new_instance
      end

      def on_unsigned_integer(*)
        typedb.unsigned_integer.new_instance
      end

      def on_float(*)
        typedb.float_type.new_instance
      end

      def on_string(*)
        typedb.string_type.new_instance
      end

      def on_template_string(node, scope)
        conv_mod = @state.module(Config::CONVERSION_MODULE)

        unless conv_mod
          diagnostics.template_strings_unavailable(node.location)
          return TypeSystem::Error.new
        end

        trait = conv_mod.lookup_type(Config::TO_STRING_CONST)

        unless trait
          diagnostics.template_strings_unavailable(node.location)
          return TypeSystem::Error.new
        end

        trait_ref = TypeSystem::Reference.new(trait)

        node.members.each do |member|
          type = define_type(member, scope)

          if !type.error? && !type.type_compatible?(trait_ref, @state)
            diagnostics.missing_to_string_trait(type, member.location)
          end
        end

        typedb.string_type.new_instance
      end

      def on_block_type(node, scope)
        proto = @state.typedb.block_type
        type =
          if node.lambda_type?
            TypeSystem::Block.lambda(proto)
          else
            TypeSystem::Block.closure(proto)
          end

        type.self_type = scope.self_type

        if !node.lambda_type? && node.moving
          type.moving = true
        end

        arg_types = node.arguments.map do |arg|
          define_type_instance(arg, scope)
        end

        type.define_arguments(arg_types)

        if node.returns
          type.return_type = define_type_instance(node.returns, scope)
        else
          type.discard_return_value = true
          type.return_type = @state.typedb.nil_type.new_instance
        end

        if node.throws
          type.throw_type = define_type_instance(node.throws, scope)
        end

        type.define_call_method

        wrap_option_type(node, type)
      end
      alias on_lambda_type on_block_type

      def on_attribute(node, scope)
        loc = node.location
        name = node.name
        symbol = scope.self_type.lookup_attribute(name)

        if symbol.nil?
          diagnostics
            .undefined_attribute_error(scope.self_type, name, loc)

          TypeSystem::Error.new
        else
          node.variable_state = scope.variable_state(name)

          diagnostics.moved_variable(name, loc) if node.variable_state.moved?

          remap_send_return_type(symbol.type.with_rigid_type_parameters, scope)
            .as_reference_or_owned(owned: scope.self_type.owned?)
        end
      end

      def on_identifier(node, scope)
        name = node.name
        loc = node.location
        self_type = scope.self_type
        depth, local = scope.depth_and_symbol_for_local(name)

        if local
          node.depth = depth
          node.symbol = local
          node.variable_state = scope.variable_state(name)

          diagnostics.moved_variable(name, loc) if node.variable_state.moved?

          local.increment_references
          remap_send_return_type(local.type, scope)
        elsif self_type.responds_to_message?(name)
          identifier_send(node, scope.self_type, name, scope)
        elsif scope.module_type.responds_to_message?(name)
          identifier_send(node, scope.module_type, name, scope)
        elsif (global = @module.lookup_global(name))
          if global.method?
            global_send(node, global, scope)
          else
            global
          end
        else
          diagnostics.undefined_method_error(self_type, name, loc)
          TypeSystem::Error.new
        end
      end

      def identifier_send(node, source, name, scope)
        method = source.lookup_method(name).type
        node.block_type = method
        return_type = method.resolved_return_type(scope.self_type)

        if method.throw_type
          node.throw_type = method.resolved_throw_type(scope.self_type)
        end

        remap_send_return_type(return_type, scope)
      end

      def global_send(node, method, scope)
        node.block_type = method
        return_type = method.resolved_return_type(scope.self_type)

        if method.throw_type
          node.throw_type = method.resolved_throw_type(scope.self_type)
        end

        remap_send_return_type(return_type, scope)
      end

      def on_self(node, scope)
        scope.self_type
      end

      def on_send(node, scope)
        receiver =
          if node.receiver
            receiver_type_for_send_with_receiver(node, scope)
          elsif scope.self_type.responds_to_message?(node.name)
            scope.self_type
          elsif scope.module_type.responds_to_message?(node.name)
            scope.module_type
          else
            nil
          end

        if receiver.nil?
          if @module.globals[node.name].any?
            return call_imported_method(node, scope)
          else
            receiver = diagnostics
              .undefined_method_error(scope.self_type, node.name, node.location)
          end
        end

        node.receiver_type = receiver

        node.type =
          if receiver.error?
            receiver
          else
            send_to_known_type(node, receiver, scope)
          end
      end

      def send_to_known_type(node, source, scope)
        name = node.name
        method = source.lookup_method(name).type_or_else do
          return diagnostics
              .undefined_method_error(source, name, node.location)
        end

        if node.receiver && method.extern
          diagnostics.external_functions_with_receiver(node.location)
          return TypeSystem::Error.new
        end

        if method.moving && source.reference?
          diagnostics.moving_method_unavailable(source, node.location)
          return TypeSystem::Error.new
        end

        if method.moving && node.receiver
          move_if_variable(node.receiver, scope)
        end

        unless verify_method_bounds(source, method, node.location)
          return TypeSystem::Error.new
        end

        exp_args = method.argument_count_range

        unless exp_args.cover?(node.arguments.length)
          return diagnostics.argument_count_error(
            node.arguments.length,
            exp_args,
            node.location
          )
        end

        method = initialize_method_for_send(node, method, scope)

        return method if method.error?

        verify_argument_types_and_initialize(node, source, method, scope)
      end

      def move_if_variable(node, scope)
        return unless node.identifier? || node.attribute?
        return unless node.type.owned?
        return unless node.variable_state
        return if copy_on_move?(node.type)

        node.variable_state.move
        scope.add_moved_variable(node.name, node.location)
      end

      def unmove_moved_variables(scope, names)
        names.each do |name|
          scope.variable_state(name)&.unmove
        end
      end

      def call_imported_method(node, scope)
        node.imported = true
        node.receiver_type = source = scope.module_type
        method = @module.globals[node.name].type
        exp_args = method.argument_count_range

        unless exp_args.cover?(node.arguments.length)
          return diagnostics.argument_count_error(
            node.arguments.length,
            exp_args,
            node.location
          )
        end

        method = initialize_method_for_send(node, method, scope)

        return method if method.error?

        verify_argument_types_and_initialize(node, source, method, scope)
      end

      def verify_method_bounds(receiver, method, loc)
        method.method_bounds.all? do |bound|
          param = receiver.lookup_type_parameter(bound.name)
          instance = receiver.lookup_type_parameter_instance(param)

          if !instance || instance.type_compatible?(bound, @state)
            true
          else
            diagnostics
              .method_requirement_error(receiver, method, instance, bound, loc)

            false
          end
        end
      end

      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      # rubocop: disable Metrics/BlockLength
      # rubocop: disable Metrics/AbcSize
      def verify_argument_types_and_initialize(node, source, method, scope)
        node.arguments.each_with_index do |arg_node, index|
          rest = false
          arg_value_node = arg_node

          if arg_node.keyword_argument?
            keyword_type = method.keyword_argument_type(arg_node.name, source)

            unless keyword_type
              return diagnostics.undefined_keyword_argument_error(
                arg_node.name,
                source,
                method,
                arg_node.location
              )
            end

            arg_value_node = arg_node.value
            exp_arg = keyword_type
          else
            exp_arg, rest = method.argument_type_at(index, source)
          end

          exp_arg = exp_arg.resolve_type_parameters(source, method)

          given_arg =
            if arg_value_node.closure? && exp_arg.block?
              # When passing a closure to a closure we want to infer the
              # arguments of our given closure according to the arguments of the
              # expected closure.
              #
              # Before we do this, we create a copy of the expected closure and
              # make sure any instance type parameters are initialised. This
              # ensures that if the expected argument of a closure is "T", we
              # use any corresponding type parameter instances if available,
              # instead of just using "T" as-is.
              #
              # In other words, if the expected block is defined like this:
              #
              #     do (T)
              #
              # And our given block is defined like this:
              #
              #     do (thing) { ... }
              #
              # Then "thing" will be whatever instance is bound to type
              # parameter "T", or "T" itself is no instance was bound.
              exp_arg =
                exp_arg.with_type_parameter_instances_from([source, method])

              # When passing a block without a signature (e.g. `foo { 10 }`) we
              # want to infer this as a lambda, if the expected block is also a
              # lambda. This allows one to write code such as the following:
              #
              #     process.spawn {
              #       ...
              #     }
              #
              # Instead of having to write this:
              #
              #     process.spawn lambda {
              #       ...
              #     }
              if arg_value_node.block_without_signature? && exp_arg.lambda?
                arg_value_node.infer_as_lambda
              end

              define_type(arg_node, scope, exp_arg)
            else
              define_type(arg_node, scope)
            end

          # When the expected argument is a rest type we need to compare
          # with/initialise the type of the individual rest values. For example,
          # for rest argument `*foo: X` the actual type of `foo` is `Array!(X)`,
          # but we want to compare with/initialise _just_ `X`.
          compare_with = rest ? type_of_rest_argument_value(exp_arg) : exp_arg

          unless given_arg.type_compatible?(compare_with, @state)
            return diagnostics.type_error(exp_arg, given_arg, arg_node.location)
          end

          move_if_variable(arg_value_node, scope) if compare_with.owned?

          compare_with.initialize_as(given_arg, method, source)
        end

        node.block_type = method
        return_type = method.resolved_return_type(source)

        if method.throw_type
          throw_type = method.resolved_throw_type(source)
          node.throw_type = remap_send_return_type(throw_type, scope)
        end

        remap_send_return_type(return_type, scope).with_rigid_type_parameters
      end
      # rubocop: enable Metrics/AbcSize
      # rubocop: enable Metrics/BlockLength
      # rubocop: enable Metrics/PerceivedComplexity
      # rubocop: enable Metrics/CyclomaticComplexity

      def initialize_method_for_send(node, method, scope)
        given = node.type_arguments.length
        max = method.type_parameters.length

        if given > max
          return diagnostics.too_many_type_parameters(max, given, node.location)
        end

        type_args = node.type_arguments.map do |type_arg_node|
          define_type_instance(type_arg_node, scope)
        end

        method.new_instance_for_send(type_args)
      end

      def remap_send_return_type(type, scope)
        if (surrounding_method = scope.enclosing_method)
          type.remap_using_method_bounds(surrounding_method)
        else
          type
        end
      end

      def receiver_type_for_send_with_receiver(node, scope)
        if node.name == Config::NEW_MESSAGE
          define_type_instance(node.receiver, scope)
        else
          define_type(node.receiver, scope)
        end
      end

      def type_of_rest_argument_value(type)
        param = type
          .lookup_type_parameter(Config::ARRAY_TYPE_PARAMETER)

        type.lookup_type_parameter_instance(param)
      end

      def on_body(node, scope)
        type =
          define_types(node.expressions, scope).last ||
          typedb.nil_type.new_instance

        # TODO: there must be a better way to do this
        last_var_name =
          if (last_expr = node.expressions.last) &&
            last_expr.is_a?(AST::Identifier) &&
            last_expr.symbol &&
            !last_expr.block_type
            last_expr.symbol.name
          end

        check_unused_locals(node.expressions)

        block_type = scope.block_type

        block_type.inferred_return_type = type if block_type.infer_return_type

        expected_type =
          block_type.return_type.resolve_self_type(scope.self_type)

        unless block_type.yield_type
          loc = node.location_of_last_expression

          if !type.never? && !type.type_compatible?(expected_type, @state)
            diagnostics.return_type_error(expected_type, type, loc)
          end

          if !type.never? && type.owned? && expected_type.reference?
            diagnostics.return_type_error(expected_type, type, loc)
          end
        end

        # TODO: is there a better way of tracking this?
        # TODO: when the last expression is a variable, we should not drop it
        # TODO: do the same when that variable is returned using `return`
        # TODO: how do we handle captured and moved owned variables?
        scope.locals.each do |local|
          state = scope.variable_state(local.name)

          next if state.nil? || state.moved?
          next if last_var_name && last_var_name == local.name

          node.drop_variables << local
        end

        type
      end

      def on_inline_body(node, scope)
        type = define_types(node.expressions, scope).last ||
          typedb.nil_type.new_instance

        node.type ||= type
      end

      def on_group(node, scope)
        define_types(node.expressions, scope).last
      end

      def on_return(node, scope)
        never = TypeSystem::Never.new
        rtype =
          if node.value
            define_type(node.value, scope)
          else
            typedb.nil_type.new_instance
          end

        # TODO: remove
        if scope.block_type.closure?
          diagnostics.warn('explicit return in closure', node.location)
        end

        block = scope.block_type

        if block
          expected = block.return_type.resolve_self_type(scope.self_type)

          if block.yield_type
            if node.value
              diagnostics.return_value_in_generator(node.location)
            end

            return never
          end

          unless rtype.type_compatible?(expected, @state)
            diagnostics
              .return_type_error(expected, rtype, node.value_location)
          end

          if block.infer_return_type
            block.inferred_return_type = rtype
          end
        else
          diagnostics.return_outside_of_method_error(node.location)
        end

        # A "return" statement itself will never return a value. For example,
        # `let x = return 10` would never assign a value to `x`.
        never
      end

      def on_yield(node, scope)
        # The value yielded is moved, so we return Nil instead.
        rtype = @state.typedb.nil_type.new_instance

        vtype =
          if node.value
            define_type(node.value, scope)
          else
            typedb.nil_type.new_instance
          end

        method = scope.enclosing_method

        unless method
          diagnostics.yield_outside_method(node.location)
          return rtype
        end

        unless method.yield_type
          diagnostics.yield_without_yield_defined(node.location)
          return rtype
        end

        unless vtype.type_compatible?(method.yield_type, @state)
          diagnostics.type_error(method.yield_type, vtype, node.value_location)
        end

        move_if_variable(node.value, scope)

        method.yields = true

        rtype
      end

      def on_try(node, scope)
        define_type(node.expression, scope)

        if node.empty_else?
          on_try_without_else(node, scope)
        else
          on_try_with_else(node, scope)
        end
      end

      def on_try_without_else(node, scope)
        ret_type = node.expression.type
        curr_block = scope.block_type

        if (throw_type = node.throw_type)
          if curr_block.infer_throw_type?
            curr_block.throw_type = throw_type
          end
        else
          diagnostics.redundant_try_warning(node.location)
        end

        ret_type
      end

      def on_try_with_else(node, scope)
        try_type = node.expression.type
        throw_type = node.throw_type || TypeSystem::Any.new
        else_scope = scope.inherit

        moved, else_type = else_scope.locals.with_unique_names do
          else_scope.record_moved_variables do
            if (else_arg_name = node.else_argument_name)
              node.else_argument_symbol =
                else_scope.define_local(else_arg_name, throw_type)
            end

            on_inline_body(node.else_body, else_scope)
          end
        end

        if node.else_body.returns? || node.else_body.throws?
          unmove_moved_variables(else_scope, moved)
        end

        if else_type.type_compatible?(try_type, @state)
          try_type
        else
          diagnostics.type_error(try_type, else_type, node.else_body.location)
        end
      end

      def on_throw(node, scope)
        type = define_type(node.value, scope)

        if scope.block_type.infer_throw_type?
          scope.block_type.throw_type = type
        end

        move_if_variable(node.value, scope)

        TypeSystem::Never.new
      end

      def on_object(node, scope)
        body_scope = scope_for_object_body(node)

        define_type(node.body, body_scope)
      end

      def on_trait(node, scope)
        body_scope = scope_for_object_body(node)

        define_type(node.body, body_scope)
      end

      def on_reopen_object(node, scope)
        type = define_type(node.name, scope)

        return type if type.error?

        unless type.object?
          return diagnostics.reopen_invalid_object_error(
            node.name.name,
            node.location
          )
        end

        block_type = TypeSystem::Block
          .closure(typedb.block_type, return_type: TypeSystem::Any.new)

        self_type = type.new_instance_with_rigid_type_parameters
        new_scope = TypeScope
          .new(self_type, block_type, @module, locals: node.body.locals)

        new_scope.define_receiver_type

        node.block_type = block_type

        define_type(node.body, new_scope)

        type
      end

      def on_trait_implementation(node, scope)
        object = define_type(node.object_name, scope)

        return object if object.error?

        # The trait name has to be looked up in the context of the
        # implementation. This ensures that a Self type refers to the type
        # that the trait is implemented for, instead of referring to the type of
        # the outer scope.
        impl_block = TypeSystem::Block
          .closure(typedb.block_type, return_type: TypeSystem::Any.new)

        self_type = object.new_instance_with_rigid_type_parameters
        impl_scope = TypeScope
          .new(self_type, impl_block, @module, locals: node.body.locals)

        impl_scope.define_receiver_type

        trait = define_type(node.trait_name, impl_scope)

        return trait if trait.error?

        # This ensures that the default methods of the trait are available on
        # the object directly. This prevents looking up the wrong type based on
        # the order in which traits are implemented.
        trait.default_methods.each do |symbol|
          if (existing = object.attributes[symbol.name]) && existing.any?
            unless existing.type.type_compatible?(symbol.type, @state)
              diagnostics.redefine_incompatible_default_method(
                trait,
                existing.type,
                symbol.type,
                node.location
              )
            end
          else
            object.attributes.define(symbol.name, symbol.type, symbol.mutable?)
          end
        end

        define_type(node.body, impl_scope)

        if trait_requirements_met?(object, trait, node.location)
          trait
        else
          object.remove_trait_implementation(trait)

          TypeSystem::Error.new
        end
      end

      def trait_requirements_met?(object, trait, location)
        required_traits_implemented?(object, trait, location) &&
          required_methods_implemented?(object, trait, location)
      end

      def required_traits_implemented?(object, trait, location)
        trait.required_trait_types.all? do |required|
          if object.implements_trait?(required)
            true
          else
            diagnostics
              .uninplemented_trait_error(trait, object, required, location)

            false
          end
        end
      end

      def required_methods_implemented?(object, trait, location)
        trait.required_methods.all? do |required|
          req_method = required.type.with_type_parameter_instances_from([trait])

          if object.implements_method?(req_method, @state)
            true
          else
            diagnostics
              .unimplemented_method_error(req_method, object, location)

            false
          end
        end
      end

      def on_method(node, scope)
        if node.arguments.length > Config::MAXIMUM_METHOD_ARGUMENTS
          diagnostics.too_many_arguments(node.location)
        end

        type = TypeSystem::Block.named_method(node.name, typedb.block_type)

        type.moving = node.move

        self_type = scope
          .self_type
          .new_instance_with_rigid_type_parameters
          .as_reference_or_owned(owned: node.move)

        new_scope =
          TypeScope.new(self_type, type, @module, locals: node.body.locals)

        define_attribute_states(scope.self_type, new_scope)
        define_method_bounds(node, new_scope)
        define_block_signature(node, new_scope)
        define_generator_signature(node, new_scope) if node.yields

        store_type(type, scope, node.location)

        self_type.type_parameters.each do |param|
          if (bounded = type.method_bounds[param.name])
            self_type.type_parameter_instances.define(param, bounded)
          end
        end

        @deferred_methods << DeferredMethod.new(node, new_scope)

        type
      end

      def on_extern_method(node, scope)
        if node.arguments.length > Config::MAXIMUM_METHOD_ARGUMENTS
          diagnostics.too_many_arguments(node.location)
        end

        type = TypeSystem::Block.named_method(node.name, typedb.block_type)
        type.extern = true

        new_scope = TypeScope.new(
          scope.self_type,
          type,
          @module,
          locals: SymbolTable.new
        )

        define_method_bounds(node, new_scope)
        define_block_signature(node, new_scope)
        define_generator_signature(node, new_scope) if node.yields

        store_type(type, scope, node.location)
      end

      def on_required_method(node, scope)
        type = TypeSystem::Block.named_method(node.name, typedb.block_type)

        type.moving = node.move

        self_type = scope
          .self_type
          .new_instance_with_rigid_type_parameters
          .as_reference_or_owned(owned: node.move)

        new_scope =
          TypeScope.new(self_type, type, @module, locals: node.body.locals)

        define_block_signature(node, new_scope)
        define_generator_signature(node, new_scope) if node.yields

        if scope.self_type.trait?
          scope.self_type.define_required_method(type)
        else
          diagnostics.define_required_method_on_non_trait_error(node.location)
        end

        type
      end

      def on_deferred_method(node, scope)
        define_type(node.body, scope)

        method = scope.block_type

        if method.yield_type && !method.yields
          diagnostics.missing_yield(method.yield_type, node.location)
        end
      end

      def on_match(node, scope)
        new_scope = scope.inherit
        location = node.location
        expr_type = define_type(node.expression, new_scope)
        bind_to = node.bind_to&.name || '__inkoc_match'
        operators_mod = @state.module(Config::OPERATORS_MODULE)

        unless operators_mod
          return @state.diagnostics.pattern_matching_unavailable(location)
        end

        unless operators_mod.lookup_type(Config::MATCH_CONST)
          return @state.diagnostics.pattern_matching_unavailable(location)
        end

        new_scope.locals.with_unique_names do
          node.bind_to_symbol = new_scope.define_local(bind_to, expr_type)
          arm_types = []
          moved_vars = Set.new

          node.arms.each do |arm|
            moved, arm_type =
              define_type(arm, new_scope, expr_type, node.bind_to_symbol)

            unmove_moved_variables(new_scope, moved)
            moved_vars.merge(moved)

            return arm_type if arm_type.error?

            arm_types << arm_type
          end

          if node.match_else
            moved, else_type = new_scope.record_moved_variables do
              define_type(node.match_else, new_scope)
            end

            moved_vars.merge(moved)
          else
            else_type = typedb.nil_type.new_instance
          end

          moved_vars.each do |name|
            new_scope.variable_state(name).move
          end

          return_type = arm_types[0] || else_type
          check_types = arm_types[1..-1] || []

          check_types << else_type

          all_compatible = check_types.all? do |type|
            type.type_compatible?(return_type, @state)
          end

          return_type = TypeSystem::Any.new unless all_compatible

          return_type || else_type
        end
      end

      def on_match_else(node, scope)
        on_inline_body(node.body, scope)
      end

      def on_match_type(node, scope, matching_type, bind_to_symbol)
        # TODO: don't support `as X` when X is a trait. Traits should only exist
        # at compile-time, as reflection/runtime checking for them is too slow
        # and best to be avoided.
        unless matching_type
          return [
            [],
            @state.diagnostics.match_type_test_unavailable(node.location)
          ]
        end

        pattern_type = define_type_instance(node.pattern, scope)
          .as_reference_or_owned(owned: matching_type.owned?)

        return [[], pattern_type] if pattern_type.error?

        bind_to_symbol.with_temporary_type(pattern_type) do
          match_guard(node.guard, scope) if node.guard
          scope.record_moved_variables { on_inline_body(node.body, scope) }
        end
      end

      def on_match_expression(node, scope, matching_type, _)
        location = node.location

        if matching_type.any?
          return [[], @state.diagnostics.pattern_match_any(location)]
        end

        operators_mod = @state.module(Config::OPERATORS_MODULE)
        match_trait = operators_mod.lookup_type(Config::MATCH_CONST)
          .new_instance([matching_type])

        node.patterns.each do |pattern|
          type = define_type(pattern, scope)

          return type if type.error?

          unless type.type_compatible?(match_trait, @state)
            return [
              [],
              @state.diagnostics.invalid_match_pattern(type, match_trait, location)
            ]
          end
        end

        match_guard(node.guard, scope) if node.guard

        scope.record_moved_variables { on_inline_body(node.body, scope) }
      end

      def match_guard(node, scope)
        guard_type = define_type(node, scope)

        unless guard_type.type_instance_of?(typedb.boolean_type)
          return diagnostics.invalid_boolean_match_pattern(node.location)
        end
      end

      def on_if(node, scope)
        rtype = nil
        moved_vars = Set.new
        unmove_after = Set.new

        node.conditions.each do |cond|
          cond_type = define_type(cond.condition, scope)
          body_scope = scope.inherit

          unless cond_type.responds_to_truthy?
            diagnostics
              .invalid_condition_type(cond_type, cond.condition.location)
          end

          moved, body_type = body_scope.locals.with_unique_names do
            body_scope.record_moved_variables do
              on_inline_body(cond.body, body_scope)
            end
          end

          unmove_after.merge(moved) if cond.body.returns? || cond.body.throws?

          unmove_moved_variables(scope, moved)
          moved_vars.merge(moved)

          if rtype && !body_type.type_compatible?(rtype, @state)
            rtype = TypeSystem::Any.new
          end

          rtype ||= body_type
        end

        if node.else_body
          else_scope = scope.inherit

          moved, else_type = else_scope.locals.with_unique_names do
            else_scope.record_moved_variables do
              on_inline_body(node.else_body, else_scope)
            end
          end

          moved_vars.merge(moved)

          unless else_type.type_compatible?(rtype, @state)
            rtype = TypeSystem::Any.new
          end
        end

        moved_vars.each do |name|
          scope.variable_state(name)&.move unless unmove_after.include?(name)
        end

        if node.else_body
          rtype || TypeSystem::Any.new
        else
          TypeSystem::Any.new
        end
      end

      def on_and(node, scope)
        left = define_type(node.left, scope)
        right = define_type(node.right, scope)

        unless left.responds_to_truthy?
          diagnostics.invalid_condition_type(left, node.left.location)
        end

        unless right.responds_to_truthy?
          diagnostics.invalid_condition_type(right, node.right.location)
        end

        @state.typedb.boolean_type.new_instance
      end

      def on_or(node, scope)
        left = define_type(node.left, scope)
        right = define_type(node.right, scope)

        unless left.responds_to_truthy?
          diagnostics.invalid_condition_type(left, node.left.location)
        end

        unless right.responds_to_truthy?
          diagnostics.invalid_condition_type(right, node.right.location)
        end

        @state.typedb.boolean_type.new_instance
      end

      def on_not(node, scope)
        expr = define_type(node.expression, scope)

        unless expr.responds_to_truthy?
          diagnostics.invalid_condition_type(expr, node.expression.location)
        end

        @state.typedb.boolean_type.new_instance
      end

      def on_loop(node, scope)
        body_scope = scope.inherit
        existing = scope.locals.symbols.map(&:name).to_set

        body_scope.locals.with_unique_names do
          enter_loop { on_inline_body(node.body, body_scope) }
        end

        # This is such a hack, but sadly due to not using a graph-based IR this
        # is the least hacky we can do :<
        body_scope.moved_variables.each do |name, loc|
          next unless body_scope.variable_state(name).moved?

          if existing.include?(name) || name.start_with?('@')
            diagnostics.moved_without_reassignment(name, loc)
          end
        end

        TypeSystem::Never.new
      end

      def on_next(node, scope)
        diagnostics.next_outside_loop(node.location) unless inside_loop?
        TypeSystem::Never.new
      end

      def on_break(node, scope)
        diagnostics.break_outside_loop(node.location) unless inside_loop?
        TypeSystem::Never.new
      end

      def on_block(node, scope, expected_block = nil)
        block_type = TypeSystem::Block.closure(
          typedb.block_type,
          return_type: @state.typedb.nil_type.new_instance
        )

        locals = node.body.locals
        block_type.moving = true if node.moving || expected_block&.moving

        new_scope = TypeScope.new(
          scope.self_type,
          block_type,
          @module,
          locals: locals,
          parent: scope
        )

        node.type_scope = new_scope

        define_block_signature(node, new_scope, expected_block)
        define_type(node.body, new_scope)

        block_type.arguments.each do |arg|
          arg.type = arg.type.with_rigid_type_parameters
        end

        new_scope.each_moved_and_captured_variable do |name, loc|
          next unless scope.variable_state(name).moved?
          next if block_type.moving

          diagnostics.moved_without_reassignment(name, loc)
        end

        if expected_block&.throw_type && !block_type.throw_type
          block_type.throw_type = TypeSystem::Never.new
        end

        block_type
      end

      def on_lambda(node, scope, expected_block = nil)
        block_type = TypeSystem::Block.lambda(
          typedb.block_type,
          return_type: @state.typedb.nil_type.new_instance
        )

        new_scope = TypeScope.new(
          @module.type,
          block_type,
          @module,
          locals: node.body.locals,
          enclosing_method: scope.enclosing_method
        )

        define_block_signature(node, new_scope, expected_block)
        define_type(node.body, new_scope)

        block_type.arguments.each do |arg|
          arg.type = arg.type.with_rigid_type_parameters
        end

        block_type
      end

      def on_define_variable(node, scope)
        return node.type if node.type

        vtype = define_type(node.value, scope)
        callback = node.variable.define_variable_visitor_method

        move_if_variable(node.value, scope)

        public_send(callback, node.variable, vtype, scope, node.mutable?)
      end

      def on_define_variable_with_explicit_type(node, scope)
        vtype = define_type(node.value, scope)
        exp_type = define_type_instance(node.value_type, scope)
        callback = node.variable.define_variable_visitor_method

        vtype =
          if vtype.type_compatible?(exp_type, @state)
            exp_type
          else
            diagnostics.type_error(exp_type, vtype, node.location)
          end

        public_send(callback, node.variable, vtype, scope, node.mutable?)
      end

      def on_define_local(node, value_type, scope, mutable = false)
        name = node.name

        if scope.locals.defined?(name)
          value_type = diagnostics
            .redefine_existing_local_error(name, node.location)
        else
          node.symbol = scope.define_local(name, value_type, mutable)
        end

        value_type
      end

      def on_define_attribute(node, scope)
        name = node.name
        vtype = define_type_instance(node.value_type, scope)

        if scope.self_type.lookup_attribute(name).any?
          diagnostics
            .redefine_existing_attribute_error(name, node.location)
        else
          scope.self_type.define_attribute(name, vtype, true)
          scope.self_type.attribute_names << name

          vtype
        end
      end

      def on_define_constant(node, value_type, scope, _)
        name = node.name

        if scope.self_type.lookup_attribute(name).any?
          value_type = diagnostics
            .redefine_existing_constant_error(name, node.location)
        else
          scope.self_type.define_attribute(name, value_type)
        end

        store_type_as_global(name, value_type, scope, node.location)

        @state.typedb.nil_type.new_instance
      end

      def on_reassign_variable(node, scope)
        callback = node.variable.reassign_variable_visitor_method
        value_type = define_type(node.value, scope)

        public_send(callback, node.variable, value_type, scope)
      end

      def on_reassign_local(node, value_type, scope)
        name = node.name
        depth, existing = scope.locals.lookup_with_parent(name)

        unless existing.any?
          return diagnostics.reassign_undefined_local_error(name, node.location)
        end

        unless existing.mutable?
          diagnostics.reassign_immutable_local_error(name, node.location)
          return existing.type
        end

        unless value_type.type_compatible?(existing.type, @state)
          diagnostics.type_error(existing.type, value_type, node.location)
        end

        node.symbol = existing
        node.depth = depth

        scope.unmove_variable(name)

        @state.typedb.nil_type.new_instance
      end

      def on_reassign_attribute(node, value_type, scope)
        name = node.name
        existing = scope.self_type.lookup_attribute(name)

        unless existing.any?
          return diagnostics
              .reassign_undefined_attribute_error(name, node.location)
        end

        existing_type = existing
          .type
          .resolve_type_parameters(scope.self_type, scope.enclosing_method)

        unless value_type.type_compatible?(existing_type, @state)
          diagnostics.type_error(existing_type, value_type, node.location)
        end

        scope.variable_state(name).unmove

        @state.typedb.nil_type.new_instance
      end

      def on_destructure_array(node, scope)
        value = define_type(node.value, scope)

        is_array = value.type_instance_of?(@state.typedb.array_type)
        is_byte_array = value.type_instance_of?(@state.typedb.byte_array_type)

        if !is_array && !is_byte_array
          pair_mod = @state.module(Config::PAIR_MODULE)
          pair_type = pair_mod&.lookup_type(Config::PAIR_TYPE)
          triple_type = pair_mod&.lookup_type(Config::TRIPLE_TYPE)

          if pair_type && value.type_instance_of?(pair_type)
            return on_destructure_pair(node, scope)
          end

          if triple_type && value.type_instance_of?(triple_type)
            return on_destructure_triple(node, scope)
          end

          diagnostics.destructure_array(node.value.location)

          return TypeSystem::Error.new
        end

        node.value_kind = :byte_array if is_byte_array

        member_type =
          if is_array
            param = value.type_parameters.first
            param_type = value
              .lookup_type_parameter_instance(param)
              .as_reference_or_owned(owned: value.owned?)
          else
            @state.typedb.integer_type.new_instance
          end

        node.variables.each do |var|
          lname = var.variable.name

          next if var.variable.ignore?

          if scope.locals.defined?(lname)
            diagnostics
              .redefine_existing_local_error(lname, var.variable.location)

            next
          end

          if var.value_type
            vtype = define_type_instance(var.value_type, scope)

            unless member_type.cast_to?(vtype, @state)
              diagnostics
                .invalid_cast_error(member_type, vtype, var.value_type.location)
            end
          else
            vtype = member_type
          end

          var.symbol = scope.define_local(lname, vtype, var.mutable?)
        end

        move_if_variable(node.value, scope)

        @state.typedb.nil_type.new_instance
      end

      def on_destructure_pair(node, scope)
        node.value_kind = :pair
        destruct_tuple(2, node, scope)
      end

      def on_destructure_triple(node, scope)
        node.value_kind = :triple
        destruct_tuple(3, node, scope)
      end

      def destruct_tuple(maximum, node, scope)
        if node.variables.length > maximum
          diagnostics.too_many_destructuring_variables(
            node.value.type,
            maximum,
            node.location
          )

          return TypeSystem::Error.new
        end

        pair_type = node.value.type

        node.variables.each_with_index do |var, index|
          lname = var.variable.name

          next if var.variable.ignore?

          if scope.locals.defined?(lname)
            diagnostics
              .redefine_existing_local_error(lname, var.variable.location)

            next
          end

          param = pair_type.type_parameters.at_index(index)
          param_type = pair_type
            .lookup_type_parameter_instance(param)

          if param_type
            param_type = param_type.as_reference_or_owned(owned: pair_type.owned?)
          else
            param_type = TypeSystem::Error.new
          end

          if var.value_type
            vtype = define_type_instance(var.value_type, scope)

            unless param_type.cast_to?(vtype, @state)
              diagnostics
                .invalid_cast_error(param_type, vtype, var.value_type.location)
            end
          else
            vtype = param_type
          end

          var.symbol = scope.define_local(lname, vtype, var.mutable?)
        end

        move_if_variable(node.value, scope)

        @state.typedb.nil_type.new_instance
      end

      def on_define_argument(arg_node, scope, default_type = nil)
        block_type = scope.block_type
        name = arg_node.name

        def_type = defined_type_for_argument(arg_node, scope)
        arg_type = determine_argument_type(
          arg_node,
          def_type,
          scope.block_type,
          default_type
        )

        symbol =
          if arg_node.rest?
            block_type.define_rest_argument(
              name,
              @state.typedb.new_array_of_type(arg_type)
            )
          else
            block_type.define_required_argument(name, arg_type)
          end

        scope.define_local(symbol.name, symbol.type.with_rigid_type_parameters)
        arg_type
      end

      def on_define_type_parameter(node, scope)
        traits = define_types(node.required_traits, scope)

        scope.self_type.define_type_parameter(node.name, traits)
      end

      def on_keyword_argument(node, scope, expected_block = nil)
        if expected_block
          define_type(node.value, scope, expected_block)
        else
          define_type(node.value, scope)
        end
      end

      def on_type_cast(node, scope)
        to_cast = define_type(node.expression, scope)
        cast_to = remap_send_return_type(
          define_type_instance(node.cast_to, scope),
          scope
        )

        cast_to = cast_to.with_rigid_type_parameters

        if to_cast.cast_to?(cast_to, @state)
          cast_to
        else
          diagnostics.invalid_cast_error(to_cast, cast_to, node.location)
        end
      end

      def on_global(node, _)
        if (symbol = @module.globals[node.name]) && symbol.any?
          symbol.type
        else
          diagnostics.undefined_constant_error(node.name, node.location)
        end
      end

      def on_new_instance(node, scope)
        object =
          if node.self_type?
            scope.self_type.base_type
          else
            scope.lookup_type(node.name)
          end

        unless object
          diagnostics.undefined_constant_error(node.name, node.location)
          return TypeSystem::Error.new
        end

        unless object.object?
          diagnostics.not_an_object(node.name, object, node.location)
          return TypeSystem::Error.new
        end

        instance = object.new_instance
        set = Set.new

        if object.builtin?
          diagnostics.invalid_new_instance(object, node.location)
          return instance
        end

        node.attributes.each do |attr|
          name = attr.name
          defined = object.lookup_attribute(name)
          given = define_type(attr.value, scope)

          move_if_variable(attr.value, scope)

          if defined.nil?
            diagnostics.undefined_attribute_error(object, name, attr.location)
            next
          end

          if set.include?(name)
            diagnostics.already_assigned_attribute(name, attr.location)
            next
          end

          set << name

          unless given.type_compatible?(defined.type, @state)
            diagnostics.type_error(defined.type, given, attr.value.location)
            next
          end

          if defined.type.type_parameter? &&
              instance.initialize_type_parameter?(defined.type)
            instance.initialize_type_parameter(defined.type, given)
          end
        end

        object.attribute_names.each do |name|
          next if set.include?(name)

          diagnostics.unassigned_attribute(name, node.location)
        end

        instance
      end

      def on_reference_value(node, scope)
        # TODO: error unless type is owned
        type = define_type(node.expression, scope)

        TypeSystem::Reference.wrap(type)
      end

      def define_block_signature(node, scope, expected_block = nil)
        define_type_parameters(node, scope)
        define_argument_types(node, scope, expected_block)
        define_throw_type(node, scope)
        define_return_type(node, scope, expected_block)

        scope.define_receiver_type
        scope.block_type.define_call_method
      end

      def define_generator_signature(node, scope)
        if node.explicit_return_type?
          diagnostics.return_and_yield(node.location)
          return
        end

        yield_type = define_type(node.yields, scope)
        throw_type = node.throws&.type || TypeSystem::Never.new

        return if yield_type.error?

        scope.block_type.yield_type = yield_type
        scope.block_type.return_type = @state
          .typedb
          .generator_type
          .new_instance([yield_type, throw_type])
      end

      def define_attribute_states(self_type, scope)
        return unless self_type.object?

        self_type.base_type.attribute_names.each do |name|
          scope.define_variable_state(name)
        end
      end

      def define_method_bounds(node, scope)
        stype = scope.self_type

        node.method_bounds.each do |bound|
          name = bound.name

          if (param = stype.lookup_type_parameter(name))
            required_traits =
              param.required_traits +
              define_types(bound.required_traits, scope)

            scope
              .block_type
              .method_bounds
              .define(name, required_traits)
          else
            diagnostics
              .undefined_type_parameter_error(stype, name, bound.location)
          end
        end
      end

      def define_type_parameters(node, scope)
        node.type_parameters.each do |param_node|
          requirements = required_traits_for_type_parameter(param_node, scope)

          scope.block_type.define_type_parameter(param_node.name, requirements)
        end
      end

      # Returns an Array containing the traits required by a type parameter.
      def required_traits_for_type_parameter(node, scope)
        requirements = []

        node.required_traits.each do |req_node|
          type = define_type(req_node, scope)

          if type&.trait?
            requirements << type
          elsif type
            diagnostics
              .invalid_type_parameter_requirement(type, req_node.location)
          else
            diagnostics.undefined_constant_error(name, req_node.location)
          end
        end

        requirements
      end

      def define_argument_types(node, scope, expected_block = nil)
        if expected_block
          define_argument_types_with_expected_block(node, scope, expected_block)
        else
          define_argument_types_without_expected_block(node, scope)
        end
      end

      def define_argument_types_without_expected_block(node, scope)
        define_types(node.arguments, scope)
      end

      def define_argument_types_with_expected_block(node, scope, expected_block)
        expected_block = expected_block.type if expected_block.reference?
        expected_args = expected_block.arguments

        node.arguments.zip(expected_args) do |arg_node, exp_arg|
          expected_type = exp_arg
            .type
            .resolve_type_parameters(scope.self_type, expected_block)

          define_type(arg_node, scope, expected_type)
        end
      end

      def define_throw_type(node, scope)
        return unless node.throws

        scope.block_type.throw_type = define_type_instance(node.throws, scope)
      end

      def define_return_type(node, scope, expected_block = nil)
        scope.block_type.return_type =
          if node.returns
            scope.block_type.infer_return_type = false
            node.returns.late_binding = true

            define_type_instance(node.returns, scope)
          else
            # If a block is directly passed as an argument, and the target
            # block doesn't care about the return type (= it's left out), we
            # also discard the return type of the input block. This allows for
            # code such as this:
            #
            #     def foo(block: do) {
            #       block.call
            #     }
            #
            #     foo { 10 }
            #     foo { 'bar' }
            if expected_block&.discard_return_value
              scope.block_type.discard_return_value = true
            end

            expected_block&.return_type || @state.typedb.nil_type.new_instance
          end
      end

      # Returns the type for an explicitly defined argument type, if any.
      def defined_type_for_argument(arg_node, scope)
        define_type_instance(arg_node.value_type, scope) if arg_node.value_type
      end

      def check_unused_locals(nodes)
        nodes.each do |node|
          next unless node.variable_definition?
          next unless node.local_variable?

          var = node.variable

          next unless var.symbol
          next if var.symbol.used?

          diagnostics.unused_local_variable(var.name, node.location)
        end
      end

      # Determines which type to use for an argument.
      def determine_argument_type(
        node,
        defined_type,
        block_type,
        default_type = nil
      )
        type =
          if defined_type
            defined_type
          elsif default_type
            default_type
          else
            diagnostics.argument_type_missing(node.location)
            TypeSystem::Error.new
          end

        type.remap_using_method_bounds(block_type)
      end

      def copy_on_move?(type)
        trait =
          @state.module(Config::CLONE_MODULE)&.lookup_type(Config::COPY_CONST)

        return false unless trait

        type.implements_trait?(trait, @state)
      end
    end
    # rubocop: enable Metrics/ClassLength
  end
end
