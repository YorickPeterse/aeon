# frozen_string_literal: true

module Inkoc
  module AST
    class Group
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expressions, :location

      def initialize(expressions, location)
        @expressions = expressions
        @location = location
      end

      def visitor_method
        :on_group
      end
    end
  end
end
