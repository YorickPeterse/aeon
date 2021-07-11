# frozen_string_literal: true

module Inkoc
  class TypeScope
    attr_reader :self_type, :block_type, :module, :locals, :parent,
                :remapped_local_types, :variable_states, :moved_variables

    def initialize(
      self_type,
      block_type,
      mod,
      locals: nil,
      parent: nil,
      enclosing_method: parent&.enclosing_method,
      variable_states: {}
    )
      @self_type = self_type
      @block_type = block_type
      @module = mod
      @locals = locals
      @parent = parent
      @enclosing_method = enclosing_method
      @variable_states = variable_states
      @moved_variables = {}
    end

    def each_moved_and_captured_variable
      @moved_variables.each do |name, location|
        yield name, location if @locals[name].nil?
      end
    end

    def define_receiver_type
      block_type.self_type = self_type
    end

    def define_variable_state(name)
      @variable_states[name] = VariableState.new
    end

    def define_local(name, type, mutable = false)
      symbol = @locals.define(name, type, mutable)
      define_variable_state(name)

      symbol
    end

    def unmove_variable(name)
      variable_state(name).unmove
      @moved_variables.delete(name)
    end

    def variable_state(name)
      source = self

      while source
        if (state = source.variable_states[name])
          return state
        end

        source = source.parent
      end

      nil
    end

    def add_moved_variable(name, location)
      @moved_variables[name] = location
    end

    def record_moved_variables
      before = @moved_variables.keys
      retval = yield
      moved = @moved_variables.keys - before

      [moved, retval]
    end

    def enclosing_method
      if @enclosing_method
        @enclosing_method
      elsif block_type.method?
        block_type
      end
    end

    def module_type
      @module.type
    end

    def depth_and_symbol_for_local(name)
      depth, local = locals.lookup_with_parent(name)

      block_type.captures = true if depth >= 0

      [depth, local] if local.any?
    end

    def closure?
      block_type.closure?
    end

    def method?
      block_type.method?
    end

    def module_scope?
      self_type.base_type == module_type
    end

    def constructor?
      if (method = enclosing_method)
        self_type.object? && method.name == Inkoc::Config::INIT_MESSAGE
      else
        false
      end
    end

    def lookup_constant(name)
      block_type.lookup_type(name) ||
        enclosing_method&.lookup_type(name) ||
        self_type.lookup_type(name) ||
        @module.lookup_type(name)
    end
    alias lookup_type lookup_constant

    def lookup_method(name)
      self_type.lookup_method(name)
        .or_else { module_type.lookup_method(name) }
    end

    def inherit
      TypeScope.new(
        self_type,
        block_type,
        self.module,
        locals: locals,
        parent: parent,
        enclosing_method: enclosing_method,
        variable_states: variable_states.dup
      )
    end
  end
end
