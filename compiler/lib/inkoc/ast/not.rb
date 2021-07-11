# frozen_string_literal: true

module Inkoc
  module AST
    class Not
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :expression, :location

      def initialize(expression, location)
        @expression = expression
        @location = location
      end

      def visitor_method
        :on_not
      end
    end
  end
end
