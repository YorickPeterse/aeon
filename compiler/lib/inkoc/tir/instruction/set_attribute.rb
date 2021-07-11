# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetAttribute
        include Predicates
        include Inspect

        attr_reader :receiver, :name, :value, :location

        def initialize(receiver, name, value, location)
          @receiver = receiver
          @name = name
          @value = value
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_set_attribute
        end
      end
    end
  end
end
