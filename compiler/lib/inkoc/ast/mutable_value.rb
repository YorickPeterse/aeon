# frozen_string_literal: true

module Inkoc
  module AST
    class MutableValue
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expression, :location

      def initialize(expression, location)
        @expression = expression
        @location = location
      end

      def visitor_method
        :on_mutable_value
      end
    end
  end
end
