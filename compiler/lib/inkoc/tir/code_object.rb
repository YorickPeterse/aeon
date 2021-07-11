# frozen_string_literal: true

module Inkoc
  module TIR
    class CodeObject
      include Inspect

      attr_reader :name, :type, :locals, :registers, :location, :blocks,
                  :code_objects, :catch_table

      def initialize(name, type, location, locals: SymbolTable.new)
        @name = name
        @type = type
        @locals = locals
        @registers = VirtualRegisters.new
        @location = location
        @blocks = []
        @code_objects = []
        @catch_table = CatchTable.new
        @basic_block_id = 0
      end

      def self_type
        type.self_type
      end

      def captures?
        type.closure?
      end

      def argument_names
        @type.arguments.names
      end

      def rest_argument?
        @type.last_argument_is_rest
      end

      def local_variables_count
        @locals.length
      end

      def registers_count
        @registers.length
      end

      def start_block
        @blocks.first
      end

      def current_block
        @blocks.last
      end

      def last_instruction
        block = current_block
        block = block.previous while block.empty? && block.previous

        block.instructions.last
      end

      def each_reachable_basic_block
        return to_enum(__method__) unless block_given?

        block = start_block

        while block
          yield block

          block = block.next
        end
      end

      def reachable_basic_block?(block)
        catch_table.jump_to?(block) ||
          block.empty? ||
          block == start_block ||
          current_block == block ||
          block.previous
      end

      def define_local(name, type, mutable)
        @locals.define(name, type, mutable)
      end

      def define_immutable_local(name, type)
        define_local(name, type, false)
      end

      def register(type)
        @registers.allocate(type)
      end

      def instruct(*args)
        instruction = current_block.instruct(*args)
        instruction.register
      end

      def add_code_object(*args, **kwargs)
        object = CodeObject.new(*args, **kwargs)
        @code_objects << object

        object
      end

      def add_basic_block(*args)
        push_basic_block(new_basic_block(*args))
      end

      def add_connected_basic_block(*args)
        push_connected_basic_block(new_basic_block(*args))
      end

      def push_basic_block(block)
        @blocks << block

        block
      end

      def push_connected_basic_block(block)
        current_block&.next = block

        @blocks << block

        block
      end

      def new_basic_block(name = new_basic_block_name, *args)
        BasicBlock.new(name, *args)
      end

      def new_basic_block_name
        id = @basic_block_id.to_s
        @basic_block_id += 1

        id
      end

      def return_type
        current_block.last_instruction.register.type
      end

      def visitor_method
        :on_code_object
      end
    end
  end
end
