# frozen_string_literal: true

module Inkoc
  module AST
    class IfCondition
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :condition, :body, :location

      def initialize(condition, body, location)
        @condition = condition
        @body = body
        @location = location
      end
    end
  end
end
