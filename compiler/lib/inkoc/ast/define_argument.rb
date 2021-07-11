# frozen_string_literal: true

module Inkoc
  module AST
    class DefineArgument
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :value_type, :rest, :location

      # name - The name of the argument.
      # value_type - The type of the argument, if any.
      # rest - If the argument is a rest argument.
      # location - The SourceLocation of the argument.
      def initialize(name, value_type, rest, location)
        @name = name
        @value_type = value_type
        @rest = rest
        @location = location
      end

      def rest?
        @rest
      end

      def visitor_method
        :on_define_argument
      end
    end
  end
end
