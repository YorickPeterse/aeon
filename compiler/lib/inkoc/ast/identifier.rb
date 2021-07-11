# frozen_string_literal: true

module Inkoc
  module AST
    class Identifier
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location

      attr_accessor :block_type, :symbol, :depth, :throw_type, :variable_state

      # name - The name of the identifier.
      # location - The SourceLocation of the identifier.
      def initialize(name, location)
        @name = name
        @location = location
        @method_type = nil
        @symbol = nil
        @depth = nil
        @throw_type = nil
        @variable_state = nil
      end

      def identifier?
        true
      end

      def visitor_method
        :on_identifier
      end

      def define_variable_visitor_method
        :on_define_local
      end

      def reassign_variable_visitor_method
        :on_reassign_local
      end

      def ignore?
        @name == '_'
      end
    end
  end
end
