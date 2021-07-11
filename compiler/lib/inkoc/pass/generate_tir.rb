# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class GenerateTir
      include VisitorMethods

      Loop = Struct.new(:start, :after)

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
        @loops = []
      end

      def enter_loop(start_name, after_name)
        @loops << Loop.new(start_name, after_name)
        retval = yield
        @loops.pop

        retval
      end

      def current_loop
        @loops.last
      end

      def run(ast)
        on_module_body(ast, @module.body)

        []
      end

      def process_imports(body)
        body.add_connected_basic_block('imports')

        mod_regs = load_modules(body)

        @module.imports.each do |import|
          register = mod_regs[import.qualified_name.to_s]

          process_node(import, register, body)
        end

        body.registers.release_all(mod_regs.values)
      end

      def load_modules(body)
        imported = Set.new
        registers = {}

        @module.imports.each do |import|
          qname = import.qualified_name

          next if imported.include?(qname.to_s)

          registers[qname.to_s] = load_module(qname, body, import.location)

          imported << qname.to_s
        end

        registers
      end

      def load_module(qname, body, location)
        name_reg = set_string(qname.to_s, body, location)
        reg = body.register(@module.type)
        result = body.instruct(:Unary, :ModuleLoad, reg, name_reg, location)

        body.registers.release(name_reg)

        result
      end

      def on_module_body(node, body)
        process_imports(@module.body)

        define_module(body)

        process_node(node, body)
      end

      def define_module(body)
        body.add_connected_basic_block('define_module')

        loc = @module.location

        mod_reg = body.register(@module.type)
        mod_name_reg = set_string(@module.name.to_s, body, loc)

        body.instruct(:Unary, :ModuleGet, mod_reg, mod_name_reg, loc)

        set_global(Config::MODULE_GLOBAL, mod_reg, body, loc)

        body.registers.release(mod_reg)
        body.registers.release(mod_name_reg)
        nil
      end

      def set_current_file_path(body, location)
        set_string(@module.location.file.path.to_s, body, location)
      end

      def on_import(import, mod_reg, body)
        source_mod = @state.module(import.qualified_name)

        import.symbols.each do |symbol|
          process_node(symbol, source_mod, mod_reg, body)
        end
      end

      def on_import_symbol(symbol, source_mod, mod_reg, body)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)
        loc = symbol.location
        symbol_reg = get_attribute(mod_reg, symbol.symbol_name.name, body, loc)

        set_global(import_as, symbol_reg, body, loc)
        body.registers.release(symbol_reg)
        nil
      end

      def on_import_self(symbol, source_mod, mod_reg, body)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)

        set_global(import_as, mod_reg, body, symbol.location)
        nil
      end

      def on_import_glob(symbol, source_mod, mod_reg, body)
        loc = symbol.location

        source_mod.attributes.each do |attribute|
          sym_name = attribute.name
          symbol_reg = get_attribute(mod_reg, sym_name, body, loc)

          set_global(sym_name, symbol_reg, body, loc)
          body.registers.release(symbol_reg)
        end
      end

      def on_body(node, body)
        body.add_connected_basic_block

        last_index = node.expressions.length - 1
        last_ins = nil

        node.expressions.each_with_index do |expr, index|
          reg = process_node(expr, body)

          # TODO: for any temporary expression, drop the reg
          # temporary means anything not assigned a variable, or used in a
          # throw, local throw, return or yield
          if node.location.file.path.to_s == '/tmp/test.inko' && index != last_index
            # Literals such as strings and integers are permanent objects, so we
            # don't need to drop these.
            case expr
            when AST::NewInstance, AST::Block, AST::Match, AST::Send
              drop_value(reg, body, node.location)
            when AST::Identifier
              drop_value(reg, body, node.location) if expr.block_type
            end
          end

          next unless reg

          if index == last_index
            # Don't release the last register just yet, otherwise we may end up
            # overwriting it when generating drop code before a return.
            last_ins = body.last_instruction
          else
            body.registers.release(reg)
          end
        end

        node.drop_variables.each do |var|
          # TODO: obtain the correct depth
          var_reg = get_local_symbol(-1, var, body, node.location)

          drop_value(var_reg, body, node.location)
          body.registers.release(var_reg)
        end

        add_explicit_return(body, last_ins)
        body.registers.release(last_ins.register) if last_ins
        nil
      end

      def on_group(node, body)
        process_nodes(node.expressions, body).last
      end

      def add_explicit_return(body, ins = nil)
        return unless body.reachable_basic_block?(body.current_block)

        loc = ins ? ins.location : body.location

        # TODO: drop() return value if discarded
        if ins && !body.type.discard_return_value && ins.register
          body.instruct(:Return, false, ins.register, loc) unless ins.return?
        else
          body.instruct(:Return, false, get_nil(body, loc), loc)
        end
      end

      def on_integer(node, body)
        set_integer(node.value, body, node.location)
      end

      def on_unsigned_integer(node, body)
        value = UnsignedInteger.new(node.value)
        register = body.register(typedb.unsigned_integer_type.new_instance)

        body.instruct(:SetLiteral, register, value, node.location)
      end

      def on_float(node, body)
        register = body.register(typedb.float_type)

        body.instruct(:SetLiteral, register, node.value, node.location)
      end

      def on_string(node, body)
        set_string(node.value, body, node.location)
      end

      def on_template_string(node, body)
        registers = node.members.map do |member|
          register = process_node(member, body)
          type = register.type

          if type.type_instance_of?(typedb.string_type)
            register
          else
            as_string = type.lookup_method('as_string')

            method =
              if as_string.any?
                as_string.type
              else
                type.lookup_method('to_string').type
              end

            send_object_message(
              register,
              method.name,
              [],
              method,
              member.type,
              body,
              member.location
            )
          end
        end

        string_concat(registers, body, node.location)
      end

      def string_concat(registers, body, location)
        result = body.register(typedb.string_type.new_instance)
        aligned = make_registers_contiguous(registers, body, location)
        first = aligned.first || result
        length = registers.length

        body.instruct(:StringConcat, result, first, length, location)

        body.registers.release_all(registers)
        body.registers.release_all(aligned)

        result
      end

      def on_self(node, body)
        get_self(body, node.location)
      end

      def on_identifier(node, body)
        name = node.name
        loc = node.location

        if node.symbol && node.depth
          get_local_symbol(node.depth, node.symbol, body, loc)
        elsif node.block_type&.extern
          extern_function_call(name, [], node.block_type, body, node.location)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, node.block_type, node.type, body, loc)
        elsif @module.responds_to_message?(name)
          send_object_message(
            get_global(Config::MODULE_GLOBAL, body, loc),
            name,
            [],
            node.block_type,
            node.type,
            body,
            loc
          )
        elsif (global = @module.lookup_global(name))
          if global.method?
            call_imported_method(name, node.block_type, [], body, loc)
          else
            get_global(name, body, loc)
          end
        else
          get_nil(body, loc)
        end
      end

      def on_attribute(node, body)
        loc = node.location

        get_attribute(get_self(body, loc), node.name, body, loc)
      end

      def on_constant(node, body)
        name = node.name
        loc = node.location

        if body.self_type.lookup_attribute(name).any?
          source = get_self(body, loc)
          result = get_attribute(source, name, body, loc)

          body.registers.release(source)

          result
        elsif @module.globals.defined?(name)
          get_global(name, body, loc)
        else
          get_nil(body, loc)
        end
      end

      def on_global(node, body)
        get_global(node.name, body, node.location)
      end

      def on_method(node, body)
        receiver = get_self(body, node.location)
        result = define_method(node, receiver, body)

        body.registers.release(receiver)

        result
      end

      def on_extern_method(node, body)
        loc = node.location
        name = set_string(node.name, body, loc)
        register = body.register(node.type)

        body.instruct(:Unary, :ExternalFunctionLoad, register, name, loc)

        body.registers.release(name)

        set_global(node.name, register, body, loc)
        register
      end

      def on_block(node, body)
        define_block(
          node.block_name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location
        )
      end

      def on_lambda(node, body)
        this_module = get_global(Config::MODULE_GLOBAL, body, node.location)
        result = define_block(
          node.block_name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location,
          this_module
        )

        body.registers.release(this_module)

        result
      end

      def define_method(node, receiver, body)
        location = node.location
        name = node.name
        block_reg = define_block(
          name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          location,
          receiver
        )

        block_reg =
          set_global_if_module_scope(receiver, name, block_reg, body, location)

        set_literal_attribute(receiver, name, block_reg, body, location)
        get_attribute(receiver, name, body, location)
      end

      def define_block(
        name,
        type,
        arguments,
        block_body,
        locals,
        body,
        location,
        receiver = TIR::VirtualRegister.reserved
      )
        code_object = body.add_code_object(name, type, location, locals: locals)

        define_block_arguments(code_object, arguments)

        on_body(block_body, code_object)

        body.instruct(
          :SetBlock,
          body.register(type),
          code_object,
          receiver,
          location
        )
      end

      def define_block_arguments(code_object, arguments)
        arguments.each do |arg|
          symbol = code_object.type.arguments[arg.name]

          if arg.rest?
            define_rest_default(code_object, symbol, arg)
          end
        end
      end

      def define_rest_default(body, local, arg)
        generate_argument_default(body, local, arg.location) do
          array_reg = body.register(typedb.new_array_of_type(arg.type))

          allocate_array(array_reg, [], body, arg.location)
        end
      end

      def generate_argument_default(body, local, location)
        body.add_connected_basic_block("#{local.name}_default")

        exists_reg = local_exists(local, body, location)

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        value = yield
        result = set_local(local, value, body, location)

        body.registers.release(value)

        result
      end

      def on_object(node, body)
        define_object(node, body, Config::OBJECT_CONST)
      end

      def on_trait(node, body)
        define_object(node, body, Config::TRAIT_CONST)
      end

      def redefine_trait(node, body)
        loc = node.location
        trait = get_global(node.name, body, loc)
        block = define_block(
          node.name,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          trait
        )

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)

        trait
      end

      def define_object(node, body, proto_name)
        name = node.name
        loc = node.location
        type = body.self_type.lookup_attribute(name).type

        proto = get_global(proto_name, body, loc)
        object_reg = body.register(type)
        object = body.instruct(:AllocatePermanent, object_reg, proto, loc)
        object = store_object_literal(object, name, body, loc)

        set_object_literal_name(object, name, body, loc)

        block = define_block(
          name,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          object
        )

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)
        body.registers.release(proto)

        object
      end

      def on_trait_implementation(node, body)
        loc = node.location
        trait = get_global(node.trait_name.type_name, body, loc)
        object = get_global(node.object_name.name, body, loc)

        implement_trait(object, trait, body, loc)

        block = define_block(
          Config::IMPL_NAME,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          object
        )

        run_block(block, [], node.type, body, loc)

        body.registers.release(object)
        body.registers.release(block)

        trait
      end

      def implement_trait(object, trait, body, loc)
        name_reg =
          set_string(Config::IMPLEMENTED_TRAITS_INSTANCE_ATTRIBUTE, body, loc)

        traits_reg = body.register(typedb.new_empty_object.new_instance)

        body.add_connected_basic_block

        true_reg = get_true(body, loc)

        body.instruct(:Binary, :GetAttributeInSelf, traits_reg, object, name_reg, loc)
        body.instruct(:GotoNextBlockIfTrue, traits_reg, loc)

        # traits object does not yet exist, create it.
        proto_reg = get_global(Config::OBJECT_CONST, body, loc)

        # create and store the "map" back in the object implementing the trait.
        body.instruct(:AllocatePermanent, traits_reg, proto_reg, loc)
        body.instruct(:SetAttribute, object, name_reg, traits_reg, loc)

        # register the trait and copy its blocks.
        body.add_connected_basic_block
        body.instruct(:CopyBlocks, object, trait, loc)

        set_attribute(traits_reg, trait, true_reg, body, loc)

        body.registers.release(name_reg)
        body.registers.release(traits_reg)
        body.registers.release(proto_reg)

        true_reg
      end

      def on_reopen_object(node, body)
        loc = node.location
        object = get_global(node.name.name, body, loc)
        block = define_block(
          Config::IMPL_NAME,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          object
        )

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)

        object
      end

      def set_object_literal_name(object, name, body, location)
        attr = Config::OBJECT_NAME_INSTANCE_ATTRIBUTE
        name_reg = set_string(name, body, location)

        set_literal_attribute(object, attr, name_reg, body, location)
        body.registers.release(name_reg)
        get_nil(body, location)
      end

      def store_object_literal(object, name, body, location)
        receiver = get_self(body, location)
        object =
          set_global_if_module_scope(receiver, name, object, body, location)

        set_literal_attribute(receiver, name, object, body, location)
        body.registers.release(receiver)

        object
      end

      def on_send(node, body)
        if node.block_type.extern
          return extern_function_call(
            node.name,
            node.arguments,
            node.block_type,
            body,
            node.location
          )
        end

        return on_imported_method(node, body) if node.imported

        receiver = receiver_for_send(node, body)

        args =
          if node.receiver &&
            send_initializes_array?(node.receiver.type, node.name)
            process_nodes(node.arguments, body)
          else
            send_arguments(node.block_type, node.arguments, body, node.location)
          end

        result = send_object_message(
          receiver,
          node.name,
          args,
          node.block_type,
          node.type,
          body,
          node.location
        )

        body.registers.release(receiver)

        result
      end

      def on_imported_method(node, body)
        type = node.block_type
        args = node.arguments

        call_imported_method(node.name, type, args, body, node.location)
      end

      def call_imported_method(name, type, arguments, body, location)
        method = get_global(name, body, location)
        register = body.register(type.return_type)
        args = send_arguments(type, arguments, body, location)
        args = make_registers_contiguous(args, body, location)
        start = args.first || register

        body.instruct(:RunBlock, method, start, args.length, type, location)
        body.instruct(:Nullary, :MoveResult, register, location)

        body.registers.release_all(args)
        body.registers.release(method)

        register
      end

      def send_arguments(block, arg_nodes, body, location)
        max_args =
          if block&.block?
            block&.argument_count_without_rest.to_i
          else
            0
          end

        args = Array.new(max_args) { TIR::VirtualRegister.reserved }
        varargs = []

        arg_nodes.each_with_index do |arg, index|
          if arg.keyword_argument? && block
            args[block.arguments[arg.name].index] =
              process_node(arg.value, body)
          elsif index < max_args
            args[index] = process_node(arg, body)
          else
            varargs.push(process_node(arg, body))
          end
        end

        if varargs.any?
          varargs_reg = body.register(
            typedb.new_array_of_type(@module.lookup_object_type.new_instance)
          )

          allocate_array(varargs_reg, varargs, body, location)
          args.push(varargs_reg)
        end

        args
      end

      def receiver_for_send(node, body)
        if node.receiver
          process_node(node.receiver, body)
        elsif node.receiver_type == @module.type
          get_global(Config::MODULE_GLOBAL, body, node.location)
        else
          get_self(body, node.location)
        end
      end

      def on_type_cast(node, body)
        process_node(node.expression, body)
      end

      def on_reference_value(node, body)
        # TODO: incr at runtime
        process_node(node.expression, body)
      end

      def on_define_variable(node, body)
        callback = node.variable.define_variable_visitor_method
        value = process_node(node.value, body)
        result = public_send(callback, node.variable, value, body)

        body.registers.release(value)

        result
      end
      alias on_define_variable_with_explicit_type on_define_variable

      def on_define_local(variable, value, body)
        set_local(variable.symbol, value, body, variable.location)
        get_nil(body, variable.location)
      end

      def set_local(symbol, value, body, location)
        body.instruct(:SetLocal, symbol, value, location)
      end

      def on_destructure_array(node, body)
        return on_destructure_pair(node, body) if node.pair?
        return on_destructure_triple(node, body) if node.triple?

        value = process_node(node.value, body)
        len = node.variables.length
        min_length = set_integer(len, body, node.location)
        length_ins = node.byte_array? ? :ByteArrayLength : :ArrayLength
        length_reg = body.register(@state.typedb.integer_type.new_instance)
        check_reg = body.register(@state.typedb.boolean_type.new_instance)

        # Check if the input array has enough values.
        body.instruct(:Unary, length_ins, length_reg, value, node.location)
        body.instruct(
          :Binary,
          :IntegerGreaterOrEqual,
          check_reg,
          length_reg,
          min_length,
          node.location
        )

        body.instruct(:GotoNextBlockIfTrue, check_reg, node.location)

        # Not enough values, panic
        panic_reg = set_string(
          "The Array or ByteArray to destructure must contain at least #{len} values",
          body,
          node.location
        )

        body.instruct(:Panic, panic_reg, node.location)

        # There are enough values
        body.add_connected_basic_block

        node.variables.each_with_index do |var, index|
          next if var.variable.ignore?

          var_reg = body.register(TypeSystem::Any.new)
          index_reg = set_integer(index, body, var.location)
          ins_name = node.byte_array? ? :ByteArrayAt : :ArrayAt

          body.instruct(:Binary, ins_name, var_reg, value, index_reg, var.location)

          set_local(var.symbol, var_reg, body, var.location)
          body.registers.release(index_reg)
          body.registers.release(var_reg)
        end

        body.registers.release(value)
        body.registers.release(length_reg)
        body.registers.release(min_length)
        body.registers.release(check_reg)
        body.registers.release(panic_reg)
        get_nil(body, node.location)
      end

      def on_destructure_pair(node, body)
        destruct_tuple(2, node, body)
      end

      def on_destructure_triple(node, body)
        destruct_tuple(3, node, body)
      end

      def destruct_tuple(length, node, body)
        value = process_node(node.value, body)

        node.variables.each_with_index do |var, index|
          next if var.variable.ignore?

          val_reg = get_attribute(value, "@#{index}", body, var.location)

          set_local(var.symbol, val_reg, body, var.location)
          body.registers.release(val_reg)
        end

        get_nil(body, node.location)
      end

      def get_local_symbol(depth, symbol, body, location)
        register = body.register(symbol.type)

        if depth >= 0
          body.instruct(:GetParentLocal, register, depth, symbol, location)
        else
          body.instruct(:GetLocal, register, symbol, location)
        end
      end

      def set_global_if_module_scope(receiver, name, value, body, location)
        if module_scope?(receiver.type)
          body.registers.release(value)

          set_global(name, value, body, location)
          get_global(name, body, location)
        else
          value
        end
      end

      def set_global(name, value, body, location)
        symbol = @module.globals[name]

        if symbol.index.negative?
          raise(
            ArgumentError,
            "Global #{name.inspect} does not exist in module #{@module.name}"
          )
        end

        body.instruct(:SetGlobal, symbol, value, location)
      end

      def get_global(name, body, location)
        symbol = @module.globals[name]
        register = body.register(symbol.type)

        if symbol.index.negative?
          raise(
            ArgumentError,
            "Global #{name.inspect} does not exist in module #{@module.name}"
          )
        end

        body.instruct(:GetGlobal, register, symbol, location)
      end

      def local_exists(symbol, body, location)
        register = body.register(typedb.boolean_type.new_instance)

        body.instruct(:LocalExists, register, symbol, location)
      end

      def on_define_attribute(node, body)
        # Defining attributes does not generate any code.
      end

      def on_reassign_attribute(variable, value, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)

        set_literal_attribute(receiver, name, value, body, loc)
        body.registers.release(receiver)
        body.registers.release(value)
        get_nil(body, loc)
      end

      def on_define_constant(variable, value, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)
        value = set_global_if_module_scope(receiver, name, value, body, loc)

        set_literal_attribute(receiver, name, value, body, loc)
        body.registers.release(receiver)
        body.registers.release(value)
        get_nil(body, loc)
      end

      def on_reassign_variable(node, body)
        callback = node.variable.reassign_variable_visitor_method
        value = process_node(node.value, body)
        result = public_send(callback, node.variable, value, body)

        body.registers.release(value)

        result
      end

      def on_reassign_local(variable, value, body)
        loc = variable.location
        depth = variable.depth
        symbol = variable.symbol

        if depth >= 0
          body.instruct(:SetParentLocal, symbol, depth, value, loc)
        else
          set_local(symbol, value, body, loc)
        end

        get_nil(body, loc)
      end

      def on_raw_instruction(node, body)
        callback = node.raw_instruction_visitor_method

        if respond_to?(callback)
          public_send(callback, node, body)
        else
          get_nil(body, node.location)
        end
      end

      def raw_nullary_instruction(name, node, body)
        reg = body.register(node.type)

        body.instruct(:Nullary, name, reg, node.location)
      end

      def raw_unary_instruction(name, node, body)
        reg = body.register(node.type)
        val = process_node(node.arguments.fetch(0), body)
        result = body.instruct(:Unary, name, reg, val, node.location)

        body.registers.release(val)

        result
      end

      def raw_binary_instruction(name, node, body)
        register = body.register(node.type)
        left = process_node(node.arguments.fetch(0), body)
        right = process_node(node.arguments.fetch(1), body)
        result =
          body.instruct(:Binary, name, register, left, right, node.location)

        body.registers.release(left)
        body.registers.release(right)

        result
      end

      def raw_ternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        result =
          body.instruct(:Ternary, name, register, one, two, three, node.location)

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)

        result
      end

      def raw_quaternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)
        result = body.instruct(
          :Quaternary,
          name,
          register,
          one,
          two,
          three,
          four,
          node.location
        )

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)
        body.registers.release(four)

        result
      end

      def raw_quinary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)
        five = process_node(node.arguments.fetch(4), body)
        result = body.instruct(
          :Quinary,
          name,
          register,
          one,
          two,
          three,
          four,
          five,
          node.location
        )

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)
        body.registers.release(four)
        body.registers.release(five)

        result
      end

      def builtin_prototype_instruction(id, node, body)
        id_reg = set_integer(id, body, node.location)
        reg = body.register(node.type)
        result = body.instruct(:Unary, :GetBuiltinPrototype, reg, id_reg, node.location)

        body.registers.release(id_reg)

        result
      end

      def on_raw_set_attribute(node, body)
        args = node.arguments
        receiver = process_node(args.fetch(0), body)
        name = process_node(args.fetch(1), body)
        value = process_node(args.fetch(2), body)

        set_attribute(receiver, name, value, body, node.location)

        body.registers.release(receiver)
        body.registers.release(name)
        body.registers.release(value)

        get_nil(body, node.location)
      end

      def on_raw_get_attribute(node, body)
        raw_binary_instruction(:GetAttribute, node, body)
      end

      def on_raw_get_attribute_in_self(node, body)
        raw_binary_instruction(:GetAttributeInSelf, node, body)
      end

      def on_raw_allocate(node, body)
        args = node.arguments
        loc = node.location
        proto = process_node(args.fetch(0), body)
        register = body.register(node.type)

        body.instruct(:Allocate, register, proto, loc)
        body.registers.release(proto)

        register
      end

      def on_raw_allocate_permanent(node, body)
        args = node.arguments
        loc = node.location
        proto = process_node(args.fetch(0), body)
        register = body.register(node.type)
        result = body.instruct(:AllocatePermanent, register, proto, loc)

        body.registers.release(proto)
        body.registers.release(register)

        result
      end

      def on_raw_object_equals(node, body)
        raw_binary_instruction(:ObjectEquals, node, body)
      end

      def on_raw_copy_blocks(node, body)
        to = process_node(node.arguments.fetch(0), body)
        from = process_node(node.arguments.fetch(1), body)
        result = body.instruct(:CopyBlocks, to, from, node.location)

        body.registers.release(to)
        body.registers.release(from)

        result
      end

      def on_raw_integer_add(node, body)
        raw_binary_instruction(:IntegerAdd, node, body)
      end

      def on_raw_integer_smaller(node, body)
        raw_binary_instruction(:IntegerSmaller, node, body)
      end

      def on_raw_integer_div(node, body)
        raw_binary_instruction(:IntegerDiv, node, body)
      end

      def on_raw_integer_mul(node, body)
        raw_binary_instruction(:IntegerMul, node, body)
      end

      def on_raw_integer_sub(node, body)
        raw_binary_instruction(:IntegerSub, node, body)
      end

      def on_raw_integer_mod(node, body)
        raw_binary_instruction(:IntegerMod, node, body)
      end

      def on_raw_integer_bitwise_and(node, body)
        raw_binary_instruction(:IntegerBitwiseAnd, node, body)
      end

      def on_raw_integer_bitwise_or(node, body)
        raw_binary_instruction(:IntegerBitwiseOr, node, body)
      end

      def on_raw_integer_bitwise_xor(node, body)
        raw_binary_instruction(:IntegerBitwiseXor, node, body)
      end

      def on_raw_integer_shift_left(node, body)
        raw_binary_instruction(:IntegerShiftLeft, node, body)
      end

      def on_raw_integer_shift_right(node, body)
        raw_binary_instruction(:IntegerShiftRight, node, body)
      end

      def on_raw_integer_greater(node, body)
        raw_binary_instruction(:IntegerGreater, node, body)
      end

      def on_raw_integer_equals(node, body)
        raw_binary_instruction(:IntegerEquals, node, body)
      end

      def on_raw_integer_greater_or_equal(node, body)
        raw_binary_instruction(:IntegerGreaterOrEqual, node, body)
      end

      def on_raw_integer_smaller_or_equal(node, body)
        raw_binary_instruction(:IntegerSmallerOrEqual, node, body)
      end

      def on_raw_float_add(node, body)
        raw_binary_instruction(:FloatAdd, node, body)
      end

      def on_raw_float_div(node, body)
        raw_binary_instruction(:FloatDiv, node, body)
      end

      def on_raw_float_mul(node, body)
        raw_binary_instruction(:FloatMul, node, body)
      end

      def on_raw_float_sub(node, body)
        raw_binary_instruction(:FloatSub, node, body)
      end

      def on_raw_float_mod(node, body)
        raw_binary_instruction(:FloatMod, node, body)
      end

      def on_raw_float_smaller(node, body)
        raw_binary_instruction(:FloatSmaller, node, body)
      end

      def on_raw_float_greater(node, body)
        raw_binary_instruction(:FloatGreater, node, body)
      end

      def on_raw_float_equals(node, body)
        raw_binary_instruction(:FloatEquals, node, body)
      end

      def on_raw_float_greater_or_equal(node, body)
        raw_binary_instruction(:FloatGreaterOrEqual, node, body)
      end

      def on_raw_float_smaller_or_equal(node, body)
        raw_binary_instruction(:FloatSmallerOrEqual, node, body)
      end

      def on_raw_get_true(node, body)
        get_true(body, node.location)
      end

      def on_raw_get_false(node, body)
        get_false(body, node.location)
      end

      def on_raw_get_nil(node, body)
        get_nil(body, node.location)
      end

      def on_raw_run_block(node, body)
        block = process_node(node.arguments.fetch(0), body)
        args =
          send_arguments(block.type, node.arguments[1..-1], body, node.location)

        return_type = block.type.block? ? block.type.return_type : block.type

        run_block(
          block,
          args,
          return_type,
          body,
          node.location
        )
      end

      def on_raw_get_string_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::STRING, node, body)
      end

      def on_raw_get_integer_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::INTEGER, node, body)
      end

      def on_raw_get_float_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FLOAT, node, body)
      end

      def on_raw_get_array_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::ARRAY, node, body)
      end

      def on_raw_get_trait_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::TRAIT, node, body)
      end

      def on_raw_get_child_process_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::CHILD_PROCESS, node, body)
      end

      def on_raw_get_block_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BLOCK, node, body)
      end

      def on_raw_array_length(node, body)
        raw_unary_instruction(:ArrayLength, node, body)
      end

      def on_raw_array_at(node, body)
        raw_binary_instruction(:ArrayAt, node, body)
      end

      def on_raw_array_set(node, body)
        register = body.register(node.type)
        array_reg = process_node(node.arguments.fetch(0), body)
        index_reg = process_node(node.arguments.fetch(1), body)
        vreg = process_node(node.arguments.fetch(2), body)
        loc = node.location

        body.registers.release(array_reg)
        body.registers.release(index_reg)
        body.registers.release(vreg)

        body.instruct(:ArraySet, register, array_reg, index_reg, vreg, loc)
      end

      def on_raw_array_remove(node, body)
        raw_binary_instruction(:ArrayRemove, node, body)
      end

      def on_raw_string_size(node, body)
        raw_unary_instruction(:StringSize, node, body)
      end

      def on_raw_string_length(node, body)
        raw_unary_instruction(:StringLength, node, body)
      end

      def on_raw_string_equals(node, body)
        raw_binary_instruction(:StringEquals, node, body)
      end

      def on_raw_string_concat(node, body)
        rec = process_node(node.arguments.fetch(0), body)
        arg = process_node(node.arguments.fetch(1), body)

        string_concat([rec, arg], body, node.location)
      end

      def on_raw_string_byte(node, body)
        raw_binary_instruction(:StringByte, node, body)
      end

      def on_raw_process_spawn(node, body)
        raw_unary_instruction(:ProcessSpawn, node, body)
      end

      def on_raw_process_send_message(node, body)
        raw_binary_instruction(:ProcessSendMessage, node, body)
      end

      def on_raw_process_receive_message(node, body)
        raw_unary_instruction(:ProcessReceiveMessage, node, body)
      end

      def on_raw_process_current(node, body)
        raw_nullary_instruction(:ProcessCurrent, node, body)
      end

      def on_raw_process_suspend_current(node, body)
        timeout = process_node(node.arguments.fetch(0), body)
        result = body.instruct(:ProcessSuspendCurrent, timeout, node.location)

        body.registers.release(timeout)

        result
      end

      def on_raw_process_terminate_current(node, body)
        body.instruct(:ProcessTerminateCurrent, node.location)

        get_nil(body, node.location)
      end

      def on_raw_get_prototype(node, body)
        raw_unary_instruction(:GetPrototype, node, body)
      end

      def on_raw_attribute_exists(node, body)
        raw_binary_instruction(:AttributeExists, node, body)
      end

      def on_raw_close(node, body)
        object = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :Close, object, node.location)
        body.registers.release(object)

        get_nil(body, node.location)
      end

      def on_raw_process_set_blocking(node, body)
        raw_unary_instruction(:ProcessSetBlocking, node, body)
      end

      def on_raw_panic(node, body)
        message = process_node(node.arguments.fetch(0), body)

        body.instruct(:Panic, message, node.location)
      end

      def on_raw_exit(node, body)
        status = process_node(node.arguments.fetch(0), body)

        body.instruct(:Exit, status, node.location)
      end

      def on_raw_byte_array_from_array(node, body)
        raw_unary_instruction(:ByteArrayFromArray, node, body)
      end

      def on_raw_byte_array_set(node, body)
        raw_ternary_instruction(:ByteArraySet, node, body)
      end

      def on_raw_byte_array_at(node, body)
        raw_binary_instruction(:ByteArrayAt, node, body)
      end

      def on_raw_byte_array_remove(node, body)
        raw_binary_instruction(:ByteArrayRemove, node, body)
      end

      def on_raw_byte_array_length(node, body)
        raw_unary_instruction(:ByteArrayLength, node, body)
      end

      def on_raw_byte_array_equals(node, body)
        raw_binary_instruction(:ByteArrayEquals, node, body)
      end

      def on_raw_get_boolean_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BOOLEAN, node, body)
      end

      def on_raw_get_nil_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::NIL, node, body)
      end

      def on_raw_get_module_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::MODULE, node, body)
      end

      def on_raw_get_ffi_library_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_LIBRARY, node, body)
      end

      def on_raw_get_ffi_function_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_FUNCTION, node, body)
      end

      def on_raw_get_ffi_pointer_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_POINTER, node, body)
      end

      def on_raw_get_ip_socket_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::IP_SOCKET, node, body)
      end

      def on_raw_get_process_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::PROCESS, node, body)
      end

      def on_raw_get_unix_socket_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::UNIX_SOCKET, node, body)
      end

      def on_raw_get_read_only_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::READ_ONLY_FILE, node, body)
      end

      def on_raw_get_write_only_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::WRITE_ONLY_FILE, node, body)
      end

      def on_raw_get_read_write_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::READ_WRITE_FILE, node, body)
      end

      def on_raw_get_hasher_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::HASHER, node, body)
      end

      def on_raw_get_byte_array_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BYTE_ARRAY, node, body)
      end

      def on_raw_get_generator_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::GENERATOR, node, body)
      end

      def on_raw_set_object_name(node, body)
        loc = node.location
        obj = process_node(node.arguments.fetch(0), body)
        val = process_node(node.arguments.fetch(1), body)
        name = set_string(Config::OBJECT_NAME_INSTANCE_ATTRIBUTE, body, loc)

        set_attribute(obj, name, val, body, loc)

        body.registers.release(obj)
        body.registers.release(val)
        body.registers.release(name)

        get_nil(body, node.location)
      end

      def on_raw_current_file_path(node, body)
        set_current_file_path(body, node.location)
      end

      def on_raw_process_add_defer_to_caller(node, body)
        raw_unary_instruction(:ProcessAddDeferToCaller, node, body)
      end

      def on_raw_process_set_pinned(node, body)
        raw_unary_instruction(:ProcessSetPinned, node, body)
      end

      def on_raw_process_identifier(node, body)
        raw_unary_instruction(:ProcessIdentifier, node, body)
      end

      def on_raw_module_load(node, body)
        raw_unary_instruction(:ModuleLoad, node, body)
      end

      def on_raw_module_get(node, body)
        raw_unary_instruction(:ModuleGet, node, body)
      end

      def on_raw_generator_resume(node, body)
        register = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :GeneratorResume, register, node.location)
      end

      def on_raw_generator_value(node, body)
        raw_unary_instruction(:GeneratorValue, node, body)
      end

      def on_raw_get_null(node, body)
        TIR::VirtualRegister.reserved
      end

      def on_raw_to_iterator(node, body)
        arg = process_node(node.arguments.fetch(0), body)

        if node.create_iterator
          result = send_object_message(
            arg,
            Config::ITER_MESSAGE,
            [],
            node.block_type,
            node.type,
            body,
            node.location
          )

          body.registers.release(arg)

          result
        else
          arg
        end
      end

      def on_return(node, body)
        location = node.location
        register =
          if node.value
            process_node(node.value, body)
          else
            get_nil(body, location)
          end

        # TODO: remove method/block return argument
        body.instruct(:Return, false, register, location)
      end

      def on_throw(node, body)
        register = process_node(node.value, body)

        body.instruct(:Throw, false, register, node.location)
        body.registers.release(register)

        get_nil(body, node.location)
      end

      def on_yield(node, body)
        register = process_node(node.value, body)

        body.instruct(:Nullary, :GeneratorYield, register, node.location)
        body.registers.release(register)

        register
      end

      def on_try(node, body)
        # A "try" without an "else" block should just re-raise the error.
        unless node.explicit_block_for_else_body?
          return process_node(node.expression, body)
        end

        error_block_name = body.new_basic_block_name
        after_block_name = body.new_basic_block_name

        catch_reg = body.register(body.type.throw_type)
        ret_reg = body.register(node.expression.type)

        # Block for running the to-try expression
        try_block = body.add_connected_basic_block
        try_reg = process_node(node.expression, body)

        body.instruct(:Unary, :CopyRegister, ret_reg, try_reg, node.location)
        body.instruct(:GotoBlock, after_block_name, node.location)

        # Block for error handling
        else_block = body.add_connected_basic_block(error_block_name)

        body.instruct(:Nullary, :MoveResult, catch_reg, node.location)

        if node.else_argument_symbol
          set_local(node.else_argument_symbol, catch_reg, body, node.location)
        end

        else_reg = on_inline_body(node.else_body, body)

        body.instruct(:Unary, :CopyRegister, ret_reg, else_reg, node.location)

        # Block for everything that comes after our "try" expression.
        body.add_connected_basic_block(after_block_name)
        body.catch_table.add_entry(try_block, else_block)

        ret_reg
      end

      def on_new_instance(node, body)
        proto_name =
          if node.self_type?
            node.type.base_type.name
          else
            node.name
          end

        proto = get_global(proto_name, body, node.location)
        instance = body.register(node.type)

        body.instruct(:Allocate, instance, proto, node.location)
        body.registers.release(proto)

        node.attributes.each do |attr|
          name = set_string(attr.name, body, attr.location)
          value = process_node(attr.value, body)

          set_attribute(instance, name, value, body, attr.location)

          body.registers.release(name)
          body.registers.release(value)
        end

        # This hack is necessary because so the last instruction is the one that
        # produces a value we can return. Without this, methods such as
        # add_explicit_return() would operate on the last set attribute, not the
        # newly created instance.
        body.instruct(:Unary, :CopyRegister, instance, instance, node.location)
      end

      def on_match(node, body)
        location = node.location
        match_expr = process_node(node.expression, body) if node.expression
        output_reg = body.register(node.type)

        body.instruct(:Nullary, :GetNil, output_reg, location)

        if node.bind_to_symbol && match_expr
          set_local(node.bind_to_symbol, match_expr, body, location)
        end

        else_block = body.new_basic_block_name
        finished_block = body.new_basic_block_name
        arm_names = node.arms.map { body.new_basic_block_name }
        last_arm_block = node.match_else ? else_block : finished_block

        node.arms.each_with_index do |arm, index|
          block_name = arm_names[index]
          next_block_name = arm_names[index + 1] || last_arm_block

          body.add_connected_basic_block(block_name)

          arm_reg = process_node(arm, body, match_expr, next_block_name)

          body.instruct(:Unary, :CopyRegister, output_reg, arm_reg, arm.location)
          body.instruct(:GotoBlock, finished_block, arm.location)

          body.registers.release(arm_reg)
        end

        if node.match_else
          body.add_connected_basic_block(else_block)

          else_reg = process_node(node.match_else, body)

          body.instruct(
            :Unary,
            :CopyRegister,
            output_reg,
            else_reg,
            node.match_else.location
          )

          body.registers.release(else_reg)
        end

        body.add_connected_basic_block(finished_block)

        output_reg
      end

      def on_match_type(node, body, match_reg, next_block_name)
        location = node.location
        body_block_name = body.new_basic_block_name
        guard_block_name = body.new_basic_block_name
        pattern_reg = process_node(node.pattern, body)
        proto_reg = body.register(new_object_type)

        if node.pattern.type.trait?
          generate_implements_trait_loop(match_reg, pattern_reg, body, location)
        else
          type_test_reg = body.register(new_object_type)

          body.instruct(:Unary, :GetPrototype, proto_reg, match_reg, location)

          body.instruct(
            :Binary,
            :ObjectEquals,
            type_test_reg,
            proto_reg,
            pattern_reg,
            location
          )

          body.instruct(:GotoNextBlockIfTrue, type_test_reg, location)
          body.registers.release(type_test_reg)
          body.registers.release(proto_reg)
        end

        if (guard = node.guard)
          match_guard(
            guard,
            body,
            body_block_name,
            guard_block_name,
            next_block_name
          )
        end

        match_case_body(node, body, body_block_name, next_block_name)
      end

      def match_guard(
        node,
        body,
        body_block_name,
        guard_block_name,
        next_block_name
      )
        location = node.location

        body.instruct(:GotoBlock, next_block_name, location)
        body.add_connected_basic_block(guard_block_name)

        guard_reg = process_node(node, body)

        body.instruct(:GotoBlockIfTrue, body_block_name, guard_reg, location)
      end

      def on_match_expression(node, body, match_reg, next_block_name)
        unless match_reg
          return on_match_expression_without_input(
            node,
            body,
            next_block_name
          )
        end

        body_block_name = body.new_basic_block_name
        guard_block_name = body.new_basic_block_name
        after_pattern_block = node.guard ? guard_block_name : body_block_name

        node.patterns.each do |pattern|
          pattern_reg = process_node(pattern, body)
          rec_type = pattern_reg.type
          method = rec_type.lookup_method(Config::MATCH_MESSAGE).type
          test_reg = run_method(
            pattern_reg,
            method.name,
            [match_reg],
            method,
            method.resolved_return_type(rec_type),
            body,
            pattern.location
          )

          body.instruct(
            :GotoBlockIfTrue,
            after_pattern_block,
            test_reg,
            pattern.location
          )

          body.registers.release(pattern_reg)
          body.registers.release(test_reg)
        end

        if (guard = node.guard)
          match_guard(
            guard,
            body,
            body_block_name,
            guard_block_name,
            next_block_name
          )
        end

        match_case_body(node, body, body_block_name, next_block_name)
      end

      def on_match_expression_without_input(node, body, next_block_name)
        body_block_name = body.new_basic_block_name
        guard_block_name = body.new_basic_block_name
        after_pattern_block = node.guard ? guard_block_name : body_block_name

        node.patterns.each do |pattern|
          pattern_reg = process_node(pattern, body)

          body.instruct(
            :GotoBlockIfTrue,
            after_pattern_block,
            pattern_reg,
            pattern.location
          )

          body.registers.release(pattern_reg)
        end

        if (guard = node.guard)
          match_guard(
            guard,
            body,
            body_block_name,
            guard_block_name,
            next_block_name
          )
        end

        match_case_body(node, body, body_block_name, next_block_name)
      end

      def on_match_else(node, body)
        process_nodes(node.body.expressions, body).last ||
          get_nil(body, node.location)
      end

      def match_case_body(node, body, body_block_name, next_block_name)
        body.instruct(:GotoBlock, next_block_name, node.location)
        body.add_connected_basic_block(body_block_name)

        process_nodes(node.body.expressions, body).last ||
          get_nil(body, node.location)
      end

      def on_if(node, body)
        after_block_name = body.new_basic_block_name
        else_block_name = body.new_basic_block_name
        result_reg = body.register(TypeSystem::Any.new)
        cond_names = node.conditions.map { body.new_basic_block_name }

        cond_names << else_block_name

        node.conditions.each_with_index do |cond, index|
          block_name = cond_names[index]
          next_block_name = cond_names[index + 1] || cond_names.last
          body_block_name = body.new_basic_block_name
          loc = cond.location

          body.add_connected_basic_block(block_name)

          cond_reg = process_node(cond.condition, body)
          cond_reg = ensure_boolean(cond_reg, body, loc)

          # Jump to the body of the condition if it passed.
          body.instruct(:GotoBlockIfTrue, body_block_name, cond_reg, loc)

          # Skip this entire block if the condition failed.
          body.instruct(:GotoBlock, next_block_name, loc)

          body.add_connected_basic_block(body_block_name)
          body.registers.release(cond_reg)

          cond_result = on_inline_body(cond.body, body)

          body.instruct(:Unary, :CopyRegister, result_reg, cond_result, loc)
          body.instruct(:GotoBlock, after_block_name, loc)

          body.registers.release(cond_result)
        end

        body.add_connected_basic_block(else_block_name)

        if node.else_body
          else_reg = on_inline_body(node.else_body, body)

          body.instruct(:Unary, :CopyRegister, result_reg, else_reg, node.location)
          body.registers.release(else_reg)
        end

        body.add_connected_basic_block(after_block_name)

        # Without an `else` we ignore whatever is returned. This is needed as
        # otherwise our return register may be uninitialised.
        unless node.else_body
          nil_reg = get_nil(body, node.location)

          body.instruct(:Unary, :CopyRegister, result_reg, nil_reg, node.location)
          body.registers.release(nil_reg)
        end

        result_reg
      end

      def on_and(node, body)
        body.add_connected_basic_block

        after_block_name = body.new_basic_block_name
        right_block_name = body.new_basic_block_name
        loc = node.location

        result = process_node(node.left, body)
        result = ensure_boolean(result, body, loc)

        body.instruct(:GotoBlockIfTrue, right_block_name, result, loc)
        body.instruct(:GotoBlock, after_block_name, loc)

        body.add_connected_basic_block(right_block_name)

        right = process_node(node.right, body)
        right = ensure_boolean(right, body, loc)

        body.instruct(:Unary, :CopyRegister, result, right, loc)
        body.registers.release(right)

        body.add_connected_basic_block(after_block_name)

        result
      end

      def on_or(node, body)
        body.add_connected_basic_block

        after_block_name = body.new_basic_block_name
        loc = node.location

        result = process_node(node.left, body)
        result = ensure_boolean(result, body, loc)

        body.instruct(:GotoBlockIfTrue, after_block_name, result, loc)

        right = process_node(node.right, body)
        right = ensure_boolean(right, body, loc)

        body.instruct(:Unary, :CopyRegister, result, right, loc)
        body.registers.release(right)

        body.add_connected_basic_block(after_block_name)

        result
      end

      def on_not(node, body)
        loc = node.location
        expr = process_node(node.expression, body)
        expr = ensure_boolean(expr, body, loc)
        false_reg = get_false(body, loc)
        result = body.register(@state.typedb.boolean_type.new_instance)

        body.instruct(:Binary, :ObjectEquals, result, expr, false_reg, loc)
        body.registers.release(false_reg)

        result
      end

      def on_loop(node, body)
        loop_start = body.new_basic_block_name
        loop_end = body.new_basic_block_name

        body.add_connected_basic_block(loop_start)

        enter_loop(loop_start, loop_end) do
          on_inline_body(node.body, body)
        end

        body.instruct(:GotoBlock, loop_start, node.location)
        body.add_connected_basic_block(loop_end)

        nil
      end

      def on_next(node, body)
        # TODO: run destructors
        body.instruct(:GotoBlock, current_loop.start, node.location)
      end

      def on_break(node, body)
        # TODO: run destructors
        body.instruct(:GotoBlock, current_loop.after, node.location)
      end

      def on_inline_body(node, body)
        # TODO: handle drop logic like on_body()
        process_nodes(node.expressions, body).last ||
          get_nil(body, node.location)
      end

      def generate_implements_trait_loop(object_reg, trait_reg, body, location)
        source_reg = body.register(new_object_type)
        traits_reg = body.register(new_object_type)
        exists_reg = body.register(new_object_type)
        name_reg = set_string(
          Config::IMPLEMENTED_TRAITS_INSTANCE_ATTRIBUTE,
          body,
          location
        )

        body.instruct(:Unary, :CopyRegister, source_reg, object_reg, location)

        loop_block = body.add_connected_basic_block

        body.instruct(
          :Binary,
          :GetAttribute,
          traits_reg,
          source_reg,
          name_reg,
          location
        )

        body.instruct(
          :Binary,
          :GetAttribute,
          exists_reg,
          traits_reg,
          trait_reg,
          location
        )

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)
        body.instruct(:Unary, :GetPrototype, source_reg, source_reg, location)

        # The prototype is something other than Nil, jump back to the start of
        # the loop and try again.
        res =
          body.instruct(:GotoBlockIfTrue, loop_block.name, source_reg, location)

        body.registers.release(source_reg)
        body.registers.release(traits_reg)
        body.registers.release(exists_reg)
        body.registers.release(name_reg)

        res
      end

      def run_block(block, args, return_type, body, location)
        type = block.type
        register = body.register(return_type)
        args = make_registers_contiguous(args, body, location)
        start = args.first || register

        body.registers.release_all(args)
        body.instruct(:RunBlock, block, start, args.length, type, location)
        body.instruct(:Nullary, :MoveResult, register, location)
      end

      def allocate_generator(rec, name, args, btype, rtype, body, loc)
        block = body.register(btype)
        name_reg = set_string(name, body, loc)
        register = body.register(rtype)
        args = make_registers_contiguous(args, body, loc)
        start = args.first || register

        body.instruct(:Binary, :GetAttribute, block, rec, name_reg, loc)

        body.instruct(
          :GeneratorAllocate,
          register,
          block,
          rec,
          start,
          args.length,
          loc
        )

        body.registers.release(block)
        body.registers.release(name_reg)
        body.registers.release_all(args)

        register
      end

      # Gets and executes a block, without using a fallback.
      #
      # rec - The register containing the receiver a message is sent to.
      # name - The name of the message being sent.
      # args - The arguments passed to the block.
      # block_type - The type of the block being executed.
      # return_type - The type being returned.
      # body - The CompiledCode object to generate the instructions in.
      # loc - The SourceLocation of the operation.
      def run_method(
        rec,
        name,
        args,
        block_type,
        return_type,
        body,
        loc
      )
        block = body.register(block_type)
        name_reg = set_string(name, body, loc)
        register = body.register(return_type)
        args = make_registers_contiguous(args, body, loc)
        start = args.first || register

        body.instruct(:Binary, :GetAttribute, block, rec, name_reg, loc)

        body.instruct(
          :RunBlockWithReceiver,
          block,
          rec,
          start,
          args.length,
          block_type,
          loc
        )

        body.instruct(:Nullary, :MoveResult, register, loc)
      end

      def send_to_self(name, block_type, return_type, body, location)
        receiver = get_self(body, location)

        send_object_message(
          receiver,
          name,
          [],
          block_type,
          return_type,
          body,
          location
        )
      end

      def extern_function_call(name, arguments, block_type, body, loc)
        args = send_arguments(block_type, arguments, body, loc)
        func_reg = get_global(name, body, loc)
        ret_reg = body.register(block_type.return_type)
        args = make_registers_contiguous(args, body, loc)
        start = args.first || ret_reg

        body.instruct(
          :ExternalFunctionCall, ret_reg, func_reg, start, args.length, loc
        )

        body.registers.release(func_reg)
        body.registers.release_all(args)

        ret_reg
      end

      def get_self(body, location)
        get_block_receiver(body, location)
      end

      def get_block_receiver(body, location)
        register = body.register(body.self_type)

        body.instruct(:Nullary, :BlockGetReceiver, register, location)
      end

      def get_nil(body, location)
        register = body.register(typedb.nil_type)

        body.instruct(:Nullary, :GetNil, register, location)
      end

      def get_true(body, location)
        register = body.register(typedb.true_type)

        body.instruct(:Nullary, :GetTrue, register, location)
      end

      def get_false(body, location)
        register = body.register(typedb.false_type)

        body.instruct(:Nullary, :GetFalse, register, location)
      end

      def set_string(value, body, location)
        register = body.register(typedb.string_type.new_instance)

        body.instruct(:SetLiteral, register, value, location)
      end

      def set_integer(value, body, location)
        register = body.register(typedb.integer_type.new_instance)

        body.instruct(:SetLiteral, register, value, location)
      end

      def send_object_message(
        rec,
        name,
        arguments,
        block_type,
        return_type,
        body,
        loc
      )
        rec_type = rec.type

        if send_initializes_array?(rec_type, name)
          send_sets_array(arguments, return_type, body, loc)
        elsif send_runs_block?(rec_type, name)
          run_block(rec, arguments, return_type, body, loc)
        elsif block_type&.yield_type
          allocate_generator(
            rec,
            name,
            arguments,
            block_type,
            return_type,
            body,
            loc
          )
        else
          run_method(
            rec,
            name,
            arguments,
            block_type,
            return_type,
            body,
            loc
          )
        end
      end

      def send_sets_array(arguments, return_type, body, location)
        register = body.register(return_type)

        allocate_array(register, arguments, body, location)
      end

      def send_initializes_array?(receiver, name)
        receiver.type_instance_of?(typedb.array_type) &&
          name == Config::NEW_MESSAGE
      end

      def send_runs_block?(receiver, name)
        is_block = receiver.block? ||
          (receiver.reference? && receiver.type.block?)

        is_block && name == Config::CALL_MESSAGE
      end

      def get_attribute(receiver, name, body, location)
        rec_type = receiver.type
        symbol = rec_type.lookup_attribute(name)
        name_reg = set_string(name, body, location)
        reg = body.register(symbol.type)

        body.registers.release(name_reg)

        body.instruct(:Binary, :GetAttribute, reg, receiver, name_reg, location)
      end

      def set_attribute(receiver, name, value, body, location)
        body.instruct(:SetAttribute, receiver, name, value, location)
      end

      def set_literal_attribute(receiver, name, value, body, location)
        name_reg = set_string(name, body, location)

        set_attribute(receiver, name_reg, value, body, location)
        body.registers.release(name_reg)

        nil
      end

      def allocate_array(register, values, body, location)
        length = values.length
        args = make_registers_contiguous(values, body, location)
        start = args.first || register

        body.registers.release_all(args)
        body.instruct(:ArrayAllocate, register, start, length, location)
      end

      def make_registers_contiguous(registers, body, location)
        return registers if registers.empty?

        last_id = registers.first.id.to_i
        move = false

        registers.each do |reg|
          # When padding arguments, the register for argument A may be greater
          # than the register for argument B. For example, the registers for two
          # arguments may be [69, 68].
          if reg.id < last_id || (reg.id - last_id) > 1
            move = true
            break
          end

          if registers.length > 1 && last_id.zero?
            move = true
            break
          end

          last_id = reg.id
        end

        if move
          new_registers = body.registers.allocate_range(registers.map(&:type))

          registers.zip(new_registers) do |old_reg, new_reg|
            body.instruct(:Unary, :CopyRegister, new_reg, old_reg, location)
          end

          new_registers
        else
          registers
        end
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def inspect
        # The default inspect is very slow, slowing down the rendering of any
        # runtime errors.
        '#<Pass::GenerateTir>'
      end

      def new_object_type
        @module.lookup_object_type.new_instance
      end

      # TODO: reconsider using hashing for dynamic dispatch
      def round_method_count(table)
        value = table.symbols.count { |s| s.type.method? }

        return value if value.zero?

        # Round to the nearest power of 2.
        value -= 1

        value |= value >> 1
        value |= value >> 2
        value |= value >> 4
        value |= value >> 8
        value |= value >> 16

        value + 1
      end

      def drop_value(value, body, location)
        if value.type.owned?
          drop = value.type.lookup_method('drop')

          # TODO: handle dropping Any, perhaps by ensuring all objects have a
          # default drop() method?
          return value if drop.nil?

          send_object_message(
            value,
            drop.name,
            [],
            drop.type,
            @state.typedb.nil_type.new_instance,
            body,
            location
          )

          # TODO: dealloc()
        else
          # TODO: decr() ref count
        end
      end

      def ensure_boolean(value, body, location)
        return value if value.type.type_instance_of?(@state.typedb.boolean_type)

        method = value.type.lookup_method(Config::TRUTHY_MESSAGE).type
        result = send_object_message(
          value,
          method.name,
          [],
          method,
          method.return_type,
          body,
          location
        )

        body.registers.release(value)
        result
      end
    end
    # rubocop: enable Metrics/ClassLength
  end
end
