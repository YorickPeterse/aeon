# frozen_string_literal: true

module Inkoc
  module Pass
    class CodeGeneration
      include VisitorMethods

      MAX_ARGUMENT_VALUE = 65535

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run
        mod = on_module(@module.body)

        [mod]
      end

      def on_module(code_object)
        loc = @module.location
        literals = Codegen::Literals.new
        name = literals.get_or_set(@module.body.name.to_s)
        file = literals.get_or_set(loc.file.path.to_s)
        body = Codegen::CompiledCode.new(name, file, loc.line)

        mod = Codegen::Module.new(@module.name, body, literals)

        process_instructions(mod.body, code_object, mod)
        assign_compiled_code_metadata(mod.body, code_object, mod)

        mod
      end

      def on_code_object(code_object, mod)
        loc = code_object.location
        name = mod.literals.get_or_set(code_object.name.to_s)
        file = mod.literals.get_or_set(loc.file.path.to_s)
        compiled_code = Codegen::CompiledCode.new(name, file, loc.line)

        process_instructions(compiled_code, code_object, mod)
        assign_compiled_code_metadata(compiled_code, code_object, mod)

        compiled_code
      end

      def process_instructions(compiled_code, code_object, mod)
        code_object.blocks.each do |basic_block|
          basic_block.instructions.each do |tir_ins|
            process_node(tir_ins, compiled_code, basic_block, mod)
          end
        end
      end

      def assign_compiled_code_metadata(compiled_code, code_object, mod)
        compiled_code.arguments = code_object.argument_names.map do |name|
          mod.literals.get_or_set(name)
        end

        compiled_code.locals = code_object.local_variables_count
        compiled_code.registers = code_object.registers_count
        compiled_code.captures = code_object.captures?

        set_catch_entries(compiled_code, code_object)
      end

      def set_catch_entries(compiled_code, code_object)
        entries = code_object.catch_table.entries

        compiled_code.catch_table = entries.map do |entry|
          start = entry.try_block.instruction_offset
          stop = entry.try_block.instruction_end
          jump_to = entry.else_block.instruction_offset

          Codegen::CatchEntry.new(start, stop, jump_to)
        end
      end

      def on_get_global(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:GetGlobal, [register, variable], tir_ins.location)
      end

      def on_get_local(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:GetLocal, [register, variable], tir_ins.location)
      end

      def on_get_parent_local(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        depth = tir_ins.depth
        variable = tir_ins.variable.index
        location = tir_ins.location

        compiled_code
          .instruct(:GetParentLocal, [register, depth, variable], location)
      end

      def on_set_parent_local(tir_ins, compiled_code, *)
        depth = tir_ins.depth
        variable = tir_ins.variable.index
        value = tir_ins.value.id
        location = tir_ins.location

        compiled_code
          .instruct(:SetParentLocal, [variable, depth, value], location)
      end

      def on_goto_next_block_if_true(tir_ins, compiled_code, basic_block, _)
        index = basic_block.next.instruction_offset
        register = tir_ins.register.id

        compiled_code.instruct(:GotoIfTrue, [index, register], tir_ins.location)
      end

      def on_goto_next_block_if_false(tir_ins, compiled_code, basic_block, _)
        index = basic_block.next.instruction_offset
        register = tir_ins.register.id

        compiled_code.instruct(:GotoIfFalse, [index, register], tir_ins.location)
      end

      def on_goto_block_if_true(tir_ins, compiled_code, basic_block, _)
        name = tir_ins.block_name
        index = nil

        basic_block.first_block.each_block do |block|
          if block.name == name
            index = block.instruction_offset
            break
          end
        end

        unless index
          raise "No basic block with the name #{name} could be found"
        end

        register = tir_ins.register.id

        compiled_code.instruct(:GotoIfTrue, [index, register], tir_ins.location)
      end

      def on_goto_block(tir_ins, compiled_code, basic_block, _)
        name = tir_ins.block_name
        index = nil

        basic_block.first_block.each_block do |block|
          if block.name == name
            index = block.instruction_offset
            break
          end
        end

        unless index
          raise "No basic block with the name #{name} could be found"
        end

        compiled_code.instruct(:Goto, [index], tir_ins.location)
      end

      def on_skip_next_block(tir_ins, compiled_code, basic_block, _)
        index = basic_block.next.next.instruction_offset

        compiled_code.instruct(:Goto, [index], tir_ins.location)
      end

      def on_local_exists(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:LocalExists, [register, variable], tir_ins.location)
      end

      def on_return(tir_ins, compiled_code, *)
        method_return = tir_ins.method_return ? 1 : 0
        register = tir_ins.register.id

        compiled_code
          .instruct(:Return, [method_return, register], tir_ins.location)
      end

      def on_throw(tir_ins, compiled_code, *)
        method_throw = tir_ins.method_throw ? 1 : 0
        register = tir_ins.register.id

        compiled_code
          .instruct(:Throw, [method_throw, register], tir_ins.location)
      end

      def on_run_block(tir_ins, compiled_code, *)
        block = tir_ins.block.id
        start = tir_ins.start.id
        amount = tir_ins.amount

        compiled_code
          .instruct(:RunBlock, [block, start, amount], tir_ins.location)
      end

      def on_external_function_call(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        func = tir_ins.function.id
        start = tir_ins.start.id
        amount = tir_ins.amount

        compiled_code.instruct(
          :ExternalFunctionCall, [reg, func, start, amount], tir_ins.location
        )
      end

      def on_run_block_with_receiver(tir_ins, compiled_code, *)
        block = tir_ins.block.id
        rec = tir_ins.receiver.id
        start = tir_ins.start.id
        amount = tir_ins.amount
        loc = tir_ins.location

        compiled_code
          .instruct(:RunBlockWithReceiver, [block, rec, start, amount], loc)
      end

      def on_generator_allocate(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        block = tir_ins.block.id
        rec = tir_ins.receiver.id
        start = tir_ins.start.id
        amount = tir_ins.amount
        loc = tir_ins.location

        compiled_code
          .instruct(:GeneratorAllocate, [reg, block, rec, start, amount], loc)
      end

      def on_tail_call(tir_ins, compiled_code, *)
        start = tir_ins.start.id
        amount = tir_ins.amount

        compiled_code.instruct(:TailCall, [start, amount], tir_ins.location)
      end

      def on_allocate_array(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        start = tir_ins.start.id
        len = tir_ins.length

        compiled_code
          .instruct(:ArrayAllocate, [register, start, len], tir_ins.location)
      end

      def on_string_concat(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        start = tir_ins.start.id
        len = tir_ins.length

        compiled_code
          .instruct(:StringConcat, [register, start, len], tir_ins.location)
      end

      def on_set_attribute(tir_ins, compiled_code, *)
        rec = tir_ins.receiver.id
        name = tir_ins.name.id
        val = tir_ins.value.id

        compiled_code
          .instruct(:SetAttribute, [rec, name, val], tir_ins.location)
      end

      def on_set_block(tir_ins, compiled_code, _, mod)
        reg = tir_ins.register.id
        code = on_code_object(tir_ins.code_object, mod)
        index = compiled_code.code_objects.add(code)
        receiver = tir_ins.receiver.id

        compiled_code
          .instruct(:SetBlock, [reg, index, receiver], tir_ins.location)
      end

      def on_set_literal(tir_ins, compiled_code, _, mod)
        lit = mod.literals.get_or_set(tir_ins.value)
        reg = tir_ins.register.id

        if lit > MAX_ARGUMENT_VALUE
          high = lit >> 16
          low = lit & 0xFFFF

          compiled_code
            .instruct(:SetLiteralWide, [reg, high, low], tir_ins.location)
        else
          compiled_code.instruct(:SetLiteral, [reg, lit], tir_ins.location)
        end
      end

      def on_allocate(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        args =
          if (proto = tir_ins.prototype)
            [reg, proto.id]
          else
            [reg]
          end

        compiled_code.instruct(:Allocate, args, tir_ins.location)
      end

      def on_allocate_permanent(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        args =
          if (proto = tir_ins.prototype)
            [reg, proto.id]
          else
            [reg]
          end

        compiled_code.instruct(:AllocatePermanent, args, tir_ins.location)
      end

      def on_set_local(tir_ins, compiled_code, *)
        var = tir_ins.variable.index
        val = tir_ins.value.id

        compiled_code.instruct(:SetLocal, [var, val], tir_ins.location)
      end

      def on_set_global(tir_ins, compiled_code, *)
        var = tir_ins.variable.index
        val = tir_ins.value.id

        compiled_code.instruct(:SetGlobal, [var, val], tir_ins.location)
      end

      def on_simple(tir_ins, compiled_code, *)
        compiled_code.instruct(tir_ins.name, [], tir_ins.location)
      end

      def on_nullary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id

        compiled_code.instruct(tir_ins.name, [reg], tir_ins.location)
      end

      def on_unary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        operand = tir_ins.operand.id

        compiled_code.instruct(tir_ins.name, [reg, operand], tir_ins.location)
      end

      def on_binary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        base = tir_ins.base.id
        other = tir_ins.other.id

        compiled_code
          .instruct(tir_ins.name, [reg, base, other], tir_ins.location)
      end

      def on_ternary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        one = tir_ins.one.id
        two = tir_ins.two.id
        three = tir_ins.three.id

        compiled_code
          .instruct(tir_ins.name, [reg, one, two, three], tir_ins.location)
      end

      def on_quaternary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        one = tir_ins.one.id
        two = tir_ins.two.id
        three = tir_ins.three.id
        four = tir_ins.four.id

        compiled_code.instruct(
          tir_ins.name,
          [reg, one, two, three, four],
          tir_ins.location
        )
      end

      def on_quinary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        one = tir_ins.one.id
        two = tir_ins.two.id
        three = tir_ins.three.id
        four = tir_ins.four.id
        five = tir_ins.five.id

        compiled_code.instruct(
          tir_ins.name,
          [reg, one, two, three, four, five],
          tir_ins.location
        )
      end

      def on_process_suspend_current(tir_ins, compiled_code, *)
        timeout = tir_ins.timeout.id

        compiled_code
          .instruct(:ProcessSuspendCurrent, [timeout], tir_ins.location)
      end

      def on_array_set(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        array = tir_ins.array.id
        index = tir_ins.index.id
        value = tir_ins.value.id

        compiled_code
          .instruct(:ArraySet, [reg, array, index, value], tir_ins.location)
      end

      def on_copy_blocks(tir_ins, compiled_code, *)
        to = tir_ins.to.id
        from = tir_ins.from.id

        compiled_code.instruct(:CopyBlocks, [to, from], tir_ins.location)
      end

      def on_panic(tir_ins, compiled_code, *)
        message = tir_ins.message.id

        compiled_code.instruct(:Panic, [message], tir_ins.location)
      end

      def on_exit(tir_ins, compiled_code, *)
        status = tir_ins.status.id

        compiled_code.instruct(:Exit, [status], tir_ins.location)
      end

      def on_process_terminate_current(tir_ins, compiled_code, *)
        compiled_code.instruct(:ProcessTerminateCurrent, [], tir_ins.location)
      end
    end
  end
end
