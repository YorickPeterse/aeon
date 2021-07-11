# frozen_string_literal: true

module Inkoc
  module AST
    class Or
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :left, :right, :location

      def initialize(left, right, location)
        @left = left
        @right = right
        @location = location
      end

      def visitor_method
        :on_or
      end
    end
  end
end
